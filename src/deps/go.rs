//! Inference of CGO-driven build inputs for Go projects.
//!
//! Go programs that wrap C libraries do so via `cgo`: a magic
//! `import "C"` line preceded by comments containing `#cgo` directives
//! that tell the toolchain which `CFLAGS` / `LDFLAGS` / `pkg-config`
//! tokens to plumb through. For nixpkgs we want to translate those into
//! `buildInputs` (the C libraries themselves) and `nativeBuildInputs`
//! (notably `pkg-config` whenever a `pkg-config:` directive is used).
//!
//! Strategy: walk every `*.go` file under the materialised source tree,
//! grep out `// #cgo ...` directive lines, then map each `pkg-config`
//! token or `-l<name>` LDFLAGS entry to its nixpkgs equivalent via a
//! conservative static lookup table.
//!
//! Pure-Go modules (no CGO) yield `(vec![], vec![])`; the caller can
//! treat that the same as "user declined inference".
//!
//! As with `rust_deps`, the table is best-effort: we only know about
//! well-known system libraries, and users can edit the generated
//! expression to add anything we missed.

use crate::types::{ExpressionInfo, Template};
use log::debug;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const LOG_TARGET: &str = "nix-template::go_deps";

/// Map a `pkg-config` module name (as it appears after
/// `// #cgo pkg-config:`) to nixpkgs `(buildInputs, nativeBuildInputs)`.
///
/// `pkg-config` itself is *always* added to nativeBuildInputs by the
/// caller whenever any pkg-config token is observed; the per-token
/// nbi field here is reserved for cases where an additional native tool
/// is required.
fn lookup_pkg_config(token: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match token {
        // OpenSSL & TLS
        "openssl" | "libssl" | "libcrypto" => Some((&["openssl"], &[])),

        // Compression
        "zlib" => Some((&["zlib"], &[])),
        "bzip2" => Some((&["bzip2"], &[])),
        "libzstd" => Some((&["zstd"], &[])),
        "liblzma" => Some((&["xz"], &[])),

        // Databases
        "sqlite3" | "libsqlite3" => Some((&["sqlite"], &[])),
        "libpq" => Some((&["postgresql"], &[])),

        // Networking / VCS
        "libcurl" => Some((&["curl"], &[])),
        "libgit2" => Some((&["libgit2"], &[])),
        "libssh2" => Some((&["libssh2"], &[])),
        "libssh" => Some((&["libssh"], &[])),

        // System integration
        "dbus-1" => Some((&["dbus"], &[])),
        "libsystemd" | "libudev" => Some((&["systemd"], &[])),
        "alsa" => Some((&["alsa-lib"], &[])),

        // Imaging / fonts
        "libpng" | "libpng16" => Some((&["libpng"], &[])),
        "libjpeg" => Some((&["libjpeg"], &[])),
        "freetype2" => Some((&["freetype"], &[])),
        "fontconfig" => Some((&["fontconfig"], &[])),

        // GUI toolkits
        "gtk+-3.0" => Some((&["gtk3"], &[])),
        "gtk+-2.0" => Some((&["gtk2"], &[])),
        "webkit2gtk-4.0" => Some((&["webkitgtk"], &[])),

        // Misc system libs
        "libxml-2.0" => Some((&["libxml2"], &[])),
        "libusb-1.0" => Some((&["libusb1"], &[])),
        "libpcap" => Some((&["libpcap"], &[])),
        "libpcre2-8" => Some((&["pcre2"], &[])),
        "ncurses" | "ncursesw" => Some((&["ncurses"], &[])),
        "vulkan" => Some((&["vulkan-loader"], &[])),
        "x11" => Some((&["xorg.libX11"], &[])),
        "yaml-0.1" => Some((&["libyaml"], &[])),

        _ => None,
    }
}

/// Map a bare `-l<name>` LDFLAGS entry (without the `-l`) to nixpkgs
/// build inputs. Unlike the pkg-config table, hitting this code path
/// does *not* imply pkg-config is required.
fn lookup_ldflag_lib(lib: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match lib {
        // OpenSSL & TLS
        "ssl" | "crypto" => Some((&["openssl"], &[])),

        // Compression
        "z" => Some((&["zlib"], &[])),
        "bz2" => Some((&["bzip2"], &[])),
        "zstd" => Some((&["zstd"], &[])),
        "lzma" => Some((&["xz"], &[])),

        // Databases
        "sqlite3" => Some((&["sqlite"], &[])),
        "pq" => Some((&["postgresql"], &[])),
        "mysqlclient" => Some((&["libmysqlclient"], &[])),

        // VCS / SSH
        "curl" => Some((&["curl"], &[])),
        "git2" => Some((&["libgit2"], &[])),
        "ssh2" => Some((&["libssh2"], &[])),
        "ssh" => Some((&["libssh"], &[])),

        // System integration
        "systemd" => Some((&["systemd"], &[])),
        "udev" => Some((&["systemd"], &[])),
        "dbus-1" => Some((&["dbus"], &[])),
        "asound" => Some((&["alsa-lib"], &[])),

        // Imaging / fonts
        "png" | "png16" => Some((&["libpng"], &[])),
        "jpeg" => Some((&["libjpeg"], &[])),
        "freetype" => Some((&["freetype"], &[])),
        "fontconfig" => Some((&["fontconfig"], &[])),

        // Misc
        "xml2" => Some((&["libxml2"], &[])),
        "usb-1.0" => Some((&["libusb1"], &[])),
        "pcap" => Some((&["libpcap"], &[])),
        "pcre2-8" => Some((&["pcre2"], &[])),
        "ncurses" | "ncursesw" => Some((&["ncurses"], &[])),
        "readline" => Some((&["readline"], &[])),
        "vulkan" => Some((&["vulkan-loader"], &[])),
        "GL" | "OpenGL" => Some((&["libGL"], &[])),
        "X11" => Some((&["xorg.libX11"], &[])),
        "yaml" => Some((&["libyaml"], &[])),

        _ => None,
    }
}

/// Output of parsing the cgo directives of a single Go file (or all
/// files in a project). The two sets are deliberately exposed separately
/// so the caller can decide whether to add `pkg-config` to nbi.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CgoDirectives {
    /// All `pkg-config:` tokens, e.g. `openssl`, `libsqlite3`. Order
    /// doesn't matter; duplicates are deduped.
    pub pkg_config_tokens: BTreeSet<String>,
    /// Bare library names from `-l...` entries in `LDFLAGS:`, e.g.
    /// `ssl`, `crypto`, `z`. The `-l` prefix is *not* included.
    pub ld_libs: BTreeSet<String>,
}

impl CgoDirectives {
    pub fn is_empty(&self) -> bool {
        self.pkg_config_tokens.is_empty() && self.ld_libs.is_empty()
    }
}

/// Parse the contents of one or more concatenated Go files and pull out
/// every `// #cgo` directive. Both `// #cgo` and `/* #cgo` styles are
/// permitted by the toolchain; we recognise both.
///
/// A directive is a comment line whose first non-whitespace token after
/// the comment marker is `#cgo`. The directive grammar (as documented at
/// <https://pkg.go.dev/cmd/cgo>):
///
/// ```text
/// // #cgo [build-constraint] CFLAGS:    -I/foo
/// // #cgo [build-constraint] LDFLAGS:   -L/bar -lssl -lcrypto
/// // #cgo [build-constraint] pkg-config: libssl libcrypto
/// ```
///
/// Build constraints (`linux,!arm64`, `darwin`, etc.) are accepted but
/// ignored — for the purposes of populating nixpkgs inputs, treating
/// every constraint as live is the conservative thing to do.
pub fn parse_cgo_directives(go_source: &str) -> CgoDirectives {
    let mut out = CgoDirectives::default();

    for raw_line in go_source.lines() {
        let line = raw_line.trim_start();
        // Strip leading comment markers; accept `//` and `/*` styles.
        let body = if let Some(rest) = line.strip_prefix("//") {
            rest
        } else if let Some(rest) = line.strip_prefix("/*") {
            rest
        } else {
            continue;
        };
        let body = body.trim_start();
        let directive = match body.strip_prefix("#cgo") {
            Some(d) => d.trim_start(),
            None => continue,
        };
        // Skip any build constraint tokens (anything before the first
        // colon-suffixed word like `LDFLAGS:` or `pkg-config:`).
        let (kind, args) = match split_directive(directive) {
            Some(pair) => pair,
            None => continue,
        };
        match kind {
            "pkg-config" => {
                for tok in args.split_whitespace() {
                    out.pkg_config_tokens.insert(tok.to_owned());
                }
            }
            "LDFLAGS" => {
                for tok in args.split_whitespace() {
                    if let Some(lib) = tok.strip_prefix("-l") {
                        out.ld_libs.insert(lib.to_owned());
                    }
                }
            }
            // CFLAGS / CXXFLAGS / CPPFLAGS / FFLAGS rarely encode useful
            // nixpkgs information (they're mostly include paths the
            // builder fills in automatically), so we ignore them.
            _ => continue,
        }
    }
    out
}

/// Find the first `KIND:` token in a directive body, where KIND is
/// either `pkg-config` or one of the FLAGS variants. Anything *before*
/// that token is a build constraint and is discarded. Returns
/// `(kind_without_colon, remainder_after_colon)`.
fn split_directive(s: &str) -> Option<(&str, &str)> {
    // Tokens separated by whitespace; we want the token that ends in
    // ':'. A trailing-colon match is sufficient: `darwin LDFLAGS: ...`
    // → `LDFLAGS` is the kind, `...` is the args.
    let mut idx = 0;
    for tok in s.split_whitespace() {
        // Locate the token in `s` so we can grab the rest after it.
        let pos = s[idx..].find(tok)? + idx;
        idx = pos + tok.len();
        if let Some(stripped) = tok.strip_suffix(':') {
            let args = s[idx..].trim();
            return Some((stripped, args));
        }
    }
    None
}

/// Convert parsed cgo directives into deduplicated, sorted
/// `(build_inputs, native_build_inputs)`. `pkg-config` is added to nbi
/// exactly once whenever any `pkg-config:` token is present.
pub fn map_cgo_to_nix(directives: &CgoDirectives) -> (Vec<String>, Vec<String>) {
    let mut bi: BTreeSet<String> = BTreeSet::new();
    let mut nbi: BTreeSet<String> = BTreeSet::new();

    if !directives.pkg_config_tokens.is_empty() {
        nbi.insert("pkg-config".to_owned());
    }

    for tok in &directives.pkg_config_tokens {
        if let Some((b, n)) = lookup_pkg_config(tok) {
            for entry in b {
                bi.insert((*entry).to_owned());
            }
            for entry in n {
                nbi.insert((*entry).to_owned());
            }
        } else {
            debug!(target: LOG_TARGET, "no nixpkgs mapping for pkg-config token: {}", tok);
        }
    }

    for lib in &directives.ld_libs {
        if let Some((b, n)) = lookup_ldflag_lib(lib) {
            for entry in b {
                bi.insert((*entry).to_owned());
            }
            for entry in n {
                nbi.insert((*entry).to_owned());
            }
        } else {
            debug!(target: LOG_TARGET, "no nixpkgs mapping for -l flag: {}", lib);
        }
    }

    (bi.into_iter().collect(), nbi.into_iter().collect())
}

/// Materialise the source tree into the Nix store. Identical strategy
/// to `rust_deps::materialise_source` — we drive `nix-build` with an
/// inline `fetchFrom*` expression and parse the resulting store path
/// from stdout.
/// Materialise the source tree referenced by `info` into the Nix store.
/// Delegates to the shared `crate::source::materialise_source`.
fn materialise_source(info: &ExpressionInfo) -> Option<PathBuf> {
    crate::source::materialise_source(info)
}

/// Recursively walk `root` and accumulate cgo directives from every
/// `*.go` file (excluding `_test.go`, since cgo deps used only by tests
/// shouldn't influence the runtime closure).
///
/// Vendored dependencies under `vendor/` are skipped — they're already
/// statically analysed by `go mod`, and including their CFLAGS would
/// over-report system deps.
pub fn scan_directory(root: &Path) -> CgoDirectives {
    let mut acc = CgoDirectives::default();
    walk(root, root, &mut acc);
    acc
}

fn walk(root: &Path, dir: &Path, acc: &mut CgoDirectives) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            debug!(target: LOG_TARGET, "cannot read {}: {}", dir.display(), e);
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            // Skip vendored deps and hidden directories.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "vendor" || name.starts_with('.') {
                continue;
            }
            walk(root, &path, acc);
        } else if file_type.is_file() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".go") || name.ends_with("_test.go") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(s) => {
                    let directives = parse_cgo_directives(&s);
                    acc.pkg_config_tokens.extend(directives.pkg_config_tokens);
                    acc.ld_libs.extend(directives.ld_libs);
                }
                Err(e) => {
                    debug!(target: LOG_TARGET, "cannot read {}: {}", path.display(), e);
                }
            }
        }
    }
    let _ = root; // currently unused; kept so future logic can show paths relative to root.
}

/// Top-level entry point: materialise the source, walk its `*.go` files
/// for `#cgo` directives, and translate them into nixpkgs inputs.
///
/// Returns `None` only on hard failures (no source, fetcher unsupported,
/// network down, etc.). A pure-Go module yields `Some((vec![], vec![]))`.
/// Core inference logic: given a path to a Go project, scan for CGO
/// directives and return inferred dependencies.
fn infer_from_source_path(source_path: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let directives = scan_directory(source_path);
    if directives.is_empty() {
        eprintln!("No CGO directives found; treating as pure-Go module.");
        return Some((Vec::new(), Vec::new()));
    }

    let (bi, nbi) = map_cgo_to_nix(&directives);
    eprintln!(
        "Inferred {} buildInputs ({:?}) and {} nativeBuildInputs ({:?})",
        bi.len(),
        bi,
        nbi.len(),
        nbi,
    );
    Some((bi, nbi))
}

/// Infer Go dependencies from a local path (for --init-flake/--init-npins).
/// This skips the materialisation step and directly scans the given path.
pub fn infer_go_dependencies_from_path(source_path: &Path) -> Option<(Vec<String>, Vec<String>)> {
    eprintln!("Scanning local Go sources for CGO directives...");
    infer_from_source_path(source_path)
}

/// Top-level entry point for remote sources: materialise the source into the
/// Nix store, then delegate to the core inference logic.
pub fn infer_go_dependencies(info: &ExpressionInfo) -> Option<(Vec<String>, Vec<String>)> {
    if info.template != Template::go {
        return None;
    }

    eprintln!("Materialising source to scan for CGO directives...");
    let source = materialise_source(info)?;
    infer_from_source_path(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pkg_config_directive() {
        let src = r#"
            package foo

            // #cgo pkg-config: libssl libcrypto
            import "C"
        "#;
        let d = parse_cgo_directives(src);
        assert!(d.pkg_config_tokens.contains("libssl"));
        assert!(d.pkg_config_tokens.contains("libcrypto"));
        assert!(d.ld_libs.is_empty());
    }

    #[test]
    fn parse_ldflags_with_dash_l() {
        let src = r#"
            // #cgo LDFLAGS: -L/usr/lib -lssl -lcrypto -lz
        "#;
        let d = parse_cgo_directives(src);
        assert!(d.ld_libs.contains("ssl"));
        assert!(d.ld_libs.contains("crypto"));
        assert!(d.ld_libs.contains("z"));
        // -L paths shouldn't show up as libs
        assert!(!d.ld_libs.iter().any(|l| l.starts_with("/")));
    }

    #[test]
    fn build_constraints_are_skipped_but_directive_still_parsed() {
        // `darwin` and `linux,!arm64` are build tags; the kind is
        // still `LDFLAGS` and the args are still `-lfoo`.
        let src = r#"
            // #cgo darwin LDFLAGS: -lssl
            // #cgo linux,!arm64 pkg-config: libssl
        "#;
        let d = parse_cgo_directives(src);
        assert!(d.ld_libs.contains("ssl"));
        assert!(d.pkg_config_tokens.contains("libssl"));
    }

    #[test]
    fn cflags_are_ignored() {
        // CFLAGS rarely encode anything we can map; ensure they don't
        // pollute the output sets.
        let src = r#"
            // #cgo CFLAGS: -I/opt/foo/include -DSOMETHING=1
            // #cgo LDFLAGS: -lfoo
        "#;
        let d = parse_cgo_directives(src);
        assert!(d.pkg_config_tokens.is_empty());
        assert_eq!(d.ld_libs.iter().cloned().collect::<Vec<_>>(), vec!["foo"]);
    }

    #[test]
    fn non_cgo_comments_are_ignored() {
        let src = r#"
            // This is a regular comment.
            // // #cgo LDFLAGS: -lnope (commented-out)
            package foo
        "#;
        let d = parse_cgo_directives(src);
        // The `// // #cgo ...` line *does* technically start with `//`
        // and after stripping the marker its trim_start body is
        // `// #cgo ...` which doesn't begin with `#cgo`, so it's
        // correctly ignored.
        assert!(d.pkg_config_tokens.is_empty());
        assert!(d.ld_libs.is_empty());
    }

    #[test]
    fn map_pkg_config_includes_pkg_config_tool() {
        let mut d = CgoDirectives::default();
        d.pkg_config_tokens.insert("openssl".to_owned());
        let (bi, nbi) = map_cgo_to_nix(&d);
        assert_eq!(bi, vec!["openssl".to_owned()]);
        assert_eq!(nbi, vec!["pkg-config".to_owned()]);
    }

    #[test]
    fn map_ldflags_does_not_force_pkg_config() {
        // -lssl alone shouldn't drag in pkg-config — the user's not
        // using pkg-config in their cgo directives.
        let mut d = CgoDirectives::default();
        d.ld_libs.insert("ssl".to_owned());
        let (bi, nbi) = map_cgo_to_nix(&d);
        assert_eq!(bi, vec!["openssl".to_owned()]);
        assert!(nbi.is_empty());
    }

    #[test]
    fn map_dedupes_across_pkg_config_and_ldflags() {
        // `pkg-config: libssl` and `LDFLAGS: -lssl` both resolve to
        // `openssl`; it should only appear once.
        let mut d = CgoDirectives::default();
        d.pkg_config_tokens.insert("libssl".to_owned());
        d.ld_libs.insert("ssl".to_owned());
        let (bi, nbi) = map_cgo_to_nix(&d);
        assert_eq!(bi, vec!["openssl".to_owned()]);
        assert!(nbi.contains(&"pkg-config".to_owned()));
    }

    #[test]
    fn map_unknown_tokens_drop_silently() {
        let mut d = CgoDirectives::default();
        d.pkg_config_tokens.insert("totally-fictional-lib".to_owned());
        d.ld_libs.insert("notreal".to_owned());
        let (bi, nbi) = map_cgo_to_nix(&d);
        // pkg-config still gets added because there *was* a pkg-config
        // directive — even if we couldn't map the token. That at least
        // hints the user that pkg-config will be needed.
        assert!(bi.is_empty());
        assert_eq!(nbi, vec!["pkg-config".to_owned()]);
    }

    #[test]
    fn empty_directive_set() {
        let d = CgoDirectives::default();
        let (bi, nbi) = map_cgo_to_nix(&d);
        assert!(bi.is_empty());
        assert!(nbi.is_empty());
    }

    #[test]
    fn end_to_end_realistic_go_source() {
        // Realistic shape of a CGO-using Go file.
        let src = r#"
package mypkg

/*
#include <openssl/ssl.h>
*/
// #cgo CFLAGS: -I/opt/openssl/include
// #cgo LDFLAGS: -L/opt/openssl/lib -lssl -lcrypto -lz
// #cgo pkg-config: libsqlite3
import "C"

func Foo() {}
"#;
        let d = parse_cgo_directives(src);
        let (bi, nbi) = map_cgo_to_nix(&d);
        assert!(bi.contains(&"openssl".to_owned()));
        assert!(bi.contains(&"sqlite".to_owned()));
        assert!(bi.contains(&"zlib".to_owned()));
        assert!(nbi.contains(&"pkg-config".to_owned()));
    }

    #[test]
    fn scan_directory_skips_vendor_and_test_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        // Real source file with cgo deps.
        std::fs::write(
            root.join("main.go"),
            "package m\n// #cgo pkg-config: libssl\nimport \"C\"\n",
        )
        .unwrap();
        // Test file: should be ignored.
        std::fs::write(
            root.join("main_test.go"),
            "package m\n// #cgo pkg-config: libsqlite3\n",
        )
        .unwrap();
        // Vendored dep: should be ignored.
        std::fs::create_dir_all(root.join("vendor/example.com/foo")).unwrap();
        std::fs::write(
            root.join("vendor/example.com/foo/bar.go"),
            "package foo\n// #cgo LDFLAGS: -lpcap\n",
        )
        .unwrap();
        let d = scan_directory(root);
        assert!(d.pkg_config_tokens.contains("libssl"));
        assert!(!d.pkg_config_tokens.contains("libsqlite3"));
        assert!(!d.ld_libs.contains("pcap"));
    }
}

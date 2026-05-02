//! Inference of Rust crate → nixpkgs build inputs.
//!
//! This module reads a Rust project's `Cargo.toml`, walks the direct
//! `[dependencies]`, `[build-dependencies]`, and target-cfg dependency
//! tables, and maps any well-known *-sys / system-binding crate name to
//! the corresponding nixpkgs `buildInputs` and `nativeBuildInputs` entry.
//!
//! The mapping is deliberately conservative: only direct dependencies are
//! inspected (no `Cargo.lock` walking) and only well-known crates are
//! mapped. Users can edit the generated expression to add anything we
//! missed.

use crate::types::{ExpressionInfo, Fetcher, Template};
use log::debug;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value;

const LOG_TARGET: &str = "nix-template::rust_deps";

/// Static mapping from a crate name to its nixpkgs (`buildInputs`,
/// `nativeBuildInputs`) requirements.
///
/// Each tuple is `(build_inputs, native_build_inputs)`. Use `&[]` for
/// "no extra entries".
fn lookup_crate(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    // Normalise underscores/dashes — crates.io permits both forms in
    // dependency keys (`tokio_postgres` vs `tokio-postgres`).
    let normalized = name.replace('_', "-");
    match normalized.as_str() {
        // OpenSSL & TLS
        "openssl-sys" | "openssl" => Some((&["openssl"], &["pkg-config"])),

        // Git/SSH
        "libgit2-sys" => Some((&["libgit2"], &["pkg-config"])),
        "libssh2-sys" => Some((&["libssh2"], &["pkg-config"])),

        // Compression
        "libz-sys" => Some((&["zlib"], &[])),
        "bzip2-sys" => Some((&["bzip2"], &[])),
        "zstd-sys" => Some((&["zstd"], &[])),
        "lzma-sys" => Some((&["xz"], &[])),

        // Databases
        "libsqlite3-sys" | "sqlite3-sys" => Some((&["sqlite"], &[])),
        "pq-sys" => Some((&["postgresql"], &[])),
        "mysqlclient-sys" => Some((&["libmysqlclient"], &[])),

        // Graphics / fonts
        "freetype-sys" => Some((&["freetype"], &["pkg-config"])),
        "expat-sys" => Some((&["expat"], &[])),
        "fontconfig-sys" => Some((&["fontconfig"], &["pkg-config"])),

        // System integration
        "dbus" => Some((&["dbus"], &["pkg-config"])),
        "alsa-sys" => Some((&["alsa-lib"], &["pkg-config"])),
        "udev" | "libudev-sys" => Some((&["udev"], &["pkg-config"])),
        "systemd" => Some((&["systemd"], &["pkg-config"])),

        // Misc system libraries (note: lookup is on the dash-normalised
        // form, so `onig_sys` and `onig-sys` both reach this arm).
        "onig-sys" => Some((&["oniguruma"], &[])),
        "librocksdb-sys" => Some((&["rocksdb"], &[])),
        "pcre2-sys" => Some((&["pcre2"], &["pkg-config"])),
        "x11" | "x11-dl" => Some((&["xorg.libX11"], &["pkg-config"])),

        // Pure build tools (only nativeBuildInputs)
        "cmake" => Some((&[], &["cmake"])),

        _ => None,
    }
}

/// Parse the given `Cargo.lock` text and return a deduplicated list of
/// every package name in the lockfile. This is how we catch *transitive*
/// system-library bindings (e.g. a project depends on `git2`, which in
/// turn pulls in `libgit2-sys`, which we *can* map to `libgit2`).
///
/// The lockfile schema we rely on is `version >= 1`:
///
/// ```toml
/// [[package]]
/// name = "openssl-sys"
/// version = "0.9.97"
/// ```
///
/// The package corresponding to the project itself is included too; the
/// caller normally won't have a mapping for it, so it's harmless.
pub fn parse_cargo_lock(cargo_lock: &str) -> Vec<String> {
    let parsed: Value = match cargo_lock.parse() {
        Ok(v) => v,
        Err(e) => {
            debug!(target: LOG_TARGET, "failed to parse Cargo.lock: {}", e);
            return Vec::new();
        }
    };

    let mut names: BTreeSet<String> = BTreeSet::new();
    if let Some(Value::Array(packages)) = parsed.get("package") {
        for pkg in packages {
            if let Some(Value::String(name)) = pkg.get("name") {
                names.insert(name.clone());
            }
        }
    }
    names.into_iter().collect()
}

/// Parse the given `Cargo.toml` text and return a deduplicated list of
/// direct dependency names (from `[dependencies]`, `[build-dependencies]`,
/// `[dev-dependencies]` is intentionally *excluded* since it's only used
/// for tests, and `[target.*.dependencies]`).
pub fn parse_cargo_dependencies(cargo_toml: &str) -> Vec<String> {
    let parsed: Value = match cargo_toml.parse() {
        Ok(v) => v,
        Err(e) => {
            debug!(target: LOG_TARGET, "failed to parse Cargo.toml: {}", e);
            return Vec::new();
        }
    };

    let mut deps: BTreeSet<String> = BTreeSet::new();

    // Top-level [dependencies] and [build-dependencies]
    for key in &["dependencies", "build-dependencies"] {
        if let Some(Value::Table(t)) = parsed.get(*key) {
            for k in t.keys() {
                deps.insert(k.clone());
            }
        }
    }

    // [target.'cfg(...)'.dependencies] and similar
    if let Some(Value::Table(targets)) = parsed.get("target") {
        for (_, target) in targets {
            if let Value::Table(target_tbl) = target {
                for key in &["dependencies", "build-dependencies"] {
                    if let Some(Value::Table(t)) = target_tbl.get(*key) {
                        for k in t.keys() {
                            deps.insert(k.clone());
                        }
                    }
                }
            }
        }
    }

    deps.into_iter().collect()
}

/// Map a list of crate names to deduplicated, sorted
/// (`build_inputs`, `native_build_inputs`).
pub fn map_crates_to_nix(crate_names: &[String]) -> (Vec<String>, Vec<String>) {
    let mut build_inputs: BTreeSet<String> = BTreeSet::new();
    let mut native_build_inputs: BTreeSet<String> = BTreeSet::new();

    for name in crate_names {
        if let Some((bi, nbi)) = lookup_crate(name) {
            for entry in bi {
                build_inputs.insert((*entry).to_owned());
            }
            for entry in nbi {
                native_build_inputs.insert((*entry).to_owned());
            }
        }
    }

    (
        build_inputs.into_iter().collect(),
        native_build_inputs.into_iter().collect(),
    )
}

/// Materialise the source tree referenced by `info` into the Nix store
/// and return the resulting `/nix/store/...-source` path.
///
/// Currently supports `fetchFromGitHub` and `fetchFromGitea`. Returns
/// `None` for fetchers we can't cleanly drive headlessly (e.g. fetchurl
/// without a known unpacked layout) or when the build fails.
fn materialise_source(info: &ExpressionInfo) -> Option<PathBuf> {
    if info.src_sha.is_empty()
        || info.src_sha.starts_with("0000000000000000000000000000000000000000000000000000")
    {
        debug!(target: LOG_TARGET, "src_sha not yet known; skipping source materialisation");
        return None;
    }

    let rev = if info.tag_prefix.is_empty() {
        info.version.clone()
    } else {
        format!("{}{}", info.tag_prefix, info.version)
    };

    let expr = match info.fetcher {
        Fetcher::github => format!(
            "(import <nixpkgs> {{}}).fetchFromGitHub {{ owner = \"{owner}\"; repo = \"{repo}\"; rev = \"{rev}\"; sha256 = \"{sha}\"; }}",
            owner = info.owner,
            repo = info.pname,
            rev = rev,
            sha = info.src_sha,
        ),
        Fetcher::gitea => format!(
            "(import <nixpkgs> {{}}).fetchFromGitea {{ domain = \"{domain}\"; owner = \"{owner}\"; repo = \"{repo}\"; rev = \"{rev}\"; sha256 = \"{sha}\"; }}",
            domain = info.domain,
            owner = info.owner,
            repo = info.pname,
            rev = rev,
            sha = info.src_sha,
        ),
        _ => {
            debug!(
                target: LOG_TARGET,
                "fetcher {:?} not supported for dependency inference",
                info.fetcher
            );
            return None;
        }
    };

    let output = Command::new("nix-build")
        .args(&["--no-out-link", "-E"])
        .arg(&expr)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            debug!(
                target: LOG_TARGET,
                "nix-build failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            return None;
        }
        Err(e) => {
            debug!(target: LOG_TARGET, "failed to invoke nix-build: {}", e);
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.trim().lines().last()?.trim().to_owned();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

/// Locate `Cargo.toml` inside an unpacked source tree. Picks the
/// top-level Cargo.toml; workspace manifests still expose project deps
/// in their root file in most real-world projects. Returns `None` if
/// no `Cargo.toml` is found.
fn find_cargo_toml(source: &Path) -> Option<PathBuf> {
    let candidate = source.join("Cargo.toml");
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Top-level entry point: given a populated `ExpressionInfo` (with a known
/// source hash), fetch the source, parse its `Cargo.toml` and (when
/// present) `Cargo.lock`, and infer `(build_inputs, native_build_inputs)`.
///
/// `Cargo.lock` is the more useful of the two because it lists *transitive*
/// dependencies — that's where `*-sys` crates almost always live. We still
/// parse `Cargo.toml` as a fallback for projects that don't ship a lockfile
/// (libraries usually don't), and we union the two crate sets so anything
/// matched by either source contributes to the result.
///
/// Logs progress to stderr; returns `None` only on hard failures
/// (network, missing manifest, etc.).
pub fn infer_rust_dependencies(info: &ExpressionInfo) -> Option<(Vec<String>, Vec<String>)> {
    if info.template != Template::rust {
        return None;
    }

    eprintln!("Materialising source to inspect Cargo.toml/Cargo.lock...");
    let source = materialise_source(info)?;
    let cargo_toml = find_cargo_toml(&source)?;

    let manifest = match std::fs::read_to_string(&cargo_toml) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", cargo_toml.display(), e);
            return None;
        }
    };

    let mut crates: BTreeSet<String> =
        parse_cargo_dependencies(&manifest).into_iter().collect();
    debug!(target: LOG_TARGET, "direct dependencies from Cargo.toml: {:?}", crates);

    // Best-effort: scan Cargo.lock for transitive crates. Missing lockfile
    // is fine — many libraries don't ship one.
    let lock_path = source.join("Cargo.lock");
    if lock_path.is_file() {
        match std::fs::read_to_string(&lock_path) {
            Ok(s) => {
                let lock_crates = parse_cargo_lock(&s);
                debug!(
                    target: LOG_TARGET,
                    "transitive crates from Cargo.lock: {} packages",
                    lock_crates.len()
                );
                crates.extend(lock_crates);
            }
            Err(e) => {
                debug!(target: LOG_TARGET, "failed to read {}: {}", lock_path.display(), e);
            }
        }
    } else {
        debug!(target: LOG_TARGET, "no Cargo.lock at {}", lock_path.display());
    }

    let crate_list: Vec<String> = crates.into_iter().collect();
    let (bi, nbi) = map_crates_to_nix(&crate_list);
    eprintln!(
        "Inferred {} buildInputs ({:?}) and {} nativeBuildInputs ({:?})",
        bi.len(),
        bi,
        nbi.len(),
        nbi,
    );
    Some((bi, nbi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_dependencies() {
        let toml = r#"
            [package]
            name = "demo"
            version = "0.1.0"

            [dependencies]
            openssl-sys = "0.9"
            serde = { version = "1", features = ["derive"] }
            tokio = "1"
        "#;
        let deps = parse_cargo_dependencies(toml);
        assert!(deps.contains(&"openssl-sys".to_owned()));
        assert!(deps.contains(&"serde".to_owned()));
        assert!(deps.contains(&"tokio".to_owned()));
    }

    #[test]
    fn parse_build_dependencies() {
        let toml = r#"
            [package]
            name = "demo"
            version = "0.1.0"

            [build-dependencies]
            cmake = "0.1"
        "#;
        let deps = parse_cargo_dependencies(toml);
        assert!(deps.contains(&"cmake".to_owned()));
    }

    #[test]
    fn parse_target_cfg_dependencies() {
        let toml = r#"
            [package]
            name = "demo"
            version = "0.1.0"

            [target.'cfg(unix)'.dependencies]
            libudev-sys = "0.1"
        "#;
        let deps = parse_cargo_dependencies(toml);
        assert!(deps.contains(&"libudev-sys".to_owned()));
    }

    #[test]
    fn dev_dependencies_are_ignored() {
        // dev-dependencies don't end up in a release artefact, so they
        // shouldn't influence buildInputs.
        let toml = r#"
            [package]
            name = "demo"
            version = "0.1.0"

            [dev-dependencies]
            openssl-sys = "0.9"
        "#;
        let deps = parse_cargo_dependencies(toml);
        assert!(!deps.contains(&"openssl-sys".to_owned()));
    }

    #[test]
    fn map_openssl() {
        let crates = vec!["openssl-sys".to_owned()];
        let (bi, nbi) = map_crates_to_nix(&crates);
        assert_eq!(bi, vec!["openssl".to_owned()]);
        assert_eq!(nbi, vec!["pkg-config".to_owned()]);
    }

    #[test]
    fn map_underscore_normalised() {
        // Cargo accepts `tokio_postgres` and `tokio-postgres` as the same
        // crate; our lookup should as well.
        let crates = vec!["onig_sys".to_owned()];
        let (bi, nbi) = map_crates_to_nix(&crates);
        assert_eq!(bi, vec!["oniguruma".to_owned()]);
        assert!(nbi.is_empty());
    }

    #[test]
    fn pkg_config_deduped_across_multiple_crates() {
        // Two crates that both require pkg-config should only contribute
        // it once to nativeBuildInputs.
        let crates = vec!["openssl-sys".to_owned(), "libgit2-sys".to_owned()];
        let (_, nbi) = map_crates_to_nix(&crates);
        assert_eq!(nbi, vec!["pkg-config".to_owned()]);
    }

    #[test]
    fn unknown_crates_ignored() {
        let crates = vec![
            "serde".to_owned(),
            "tokio".to_owned(),
            "totally-fictional-crate".to_owned(),
        ];
        let (bi, nbi) = map_crates_to_nix(&crates);
        assert!(bi.is_empty());
        assert!(nbi.is_empty());
    }

    #[test]
    fn empty_input() {
        let (bi, nbi) = map_crates_to_nix(&[]);
        assert!(bi.is_empty());
        assert!(nbi.is_empty());
    }

    #[test]
    fn parse_cargo_lock_extracts_transitive_packages() {
        // A minimal but realistic Cargo.lock fragment. Note that
        // `openssl-sys` is *not* in the manifest's `[dependencies]`
        // (it's transitively pulled in via `git2`), but we should
        // still see it in the lock-derived list.
        let lock = r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"
dependencies = [
 "git2",
]

[[package]]
name = "git2"
version = "0.18.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abcd"
dependencies = [
 "libgit2-sys",
 "openssl-sys",
]

[[package]]
name = "libgit2-sys"
version = "0.16.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ef01"
dependencies = [
 "cc",
 "libc",
 "libssh2-sys",
 "libz-sys",
 "openssl-sys",
 "pkg-config",
]

[[package]]
name = "openssl-sys"
version = "0.9.97"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "2345"
dependencies = [
 "cc",
 "libc",
 "pkg-config",
 "vcpkg",
]

[[package]]
name = "libssh2-sys"
version = "0.3.0"

[[package]]
name = "libz-sys"
version = "1.1.0"
"#;
        let pkgs = parse_cargo_lock(lock);
        assert!(pkgs.contains(&"git2".to_owned()));
        assert!(pkgs.contains(&"libgit2-sys".to_owned()));
        assert!(pkgs.contains(&"libssh2-sys".to_owned()));
        assert!(pkgs.contains(&"libz-sys".to_owned()));
        assert!(pkgs.contains(&"openssl-sys".to_owned()));
        // The crate's own package entry is included; that's fine.
        assert!(pkgs.contains(&"demo".to_owned()));
    }

    #[test]
    fn parse_cargo_lock_handles_empty_or_malformed() {
        // Empty file → empty result.
        assert!(parse_cargo_lock("").is_empty());
        // Malformed TOML → empty result rather than panicking.
        assert!(parse_cargo_lock("this is :: not toml [[").is_empty());
        // No `[[package]]` table → empty result.
        assert!(parse_cargo_lock("version = 3\n").is_empty());
    }

    #[test]
    fn lockfile_unlocks_transitive_sys_crates() {
        // Cargo.toml only mentions `git2`, but the *lockfile* exposes the
        // transitive `libgit2-sys`/`openssl-sys`/`libssh2-sys` that we
        // can actually map. Combining the two sources catches them.
        let manifest = r#"
            [package]
            name = "demo"
            version = "0.1.0"

            [dependencies]
            git2 = "0.18"
        "#;
        let lock = r#"
[[package]]
name = "demo"
version = "0.1.0"

[[package]]
name = "git2"
version = "0.18.0"

[[package]]
name = "libgit2-sys"
version = "0.16.0"

[[package]]
name = "openssl-sys"
version = "0.9.97"

[[package]]
name = "libssh2-sys"
version = "0.3.0"
"#;
        let mut crates: BTreeSet<String> =
            parse_cargo_dependencies(manifest).into_iter().collect();
        crates.extend(parse_cargo_lock(lock));
        let crate_list: Vec<String> = crates.into_iter().collect();
        let (bi, nbi) = map_crates_to_nix(&crate_list);
        assert!(bi.contains(&"libgit2".to_owned()));
        assert!(bi.contains(&"libssh2".to_owned()));
        assert!(bi.contains(&"openssl".to_owned()));
        // pkg-config should appear exactly once even though three crates ask for it.
        assert_eq!(
            nbi.iter().filter(|n| *n == "pkg-config").count(),
            1,
            "pkg-config should be deduplicated, got: {:?}",
            nbi
        );
    }
}

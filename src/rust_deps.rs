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
/// source hash), fetch the source, parse its `Cargo.toml`, and infer
/// `(build_inputs, native_build_inputs)`. Logs progress to stderr; returns
/// `None` only on hard failures (network, malformed manifest, etc.).
pub fn infer_rust_dependencies(info: &ExpressionInfo) -> Option<(Vec<String>, Vec<String>)> {
    if info.template != Template::rust {
        return None;
    }

    eprintln!("Materialising source to inspect Cargo.toml...");
    let source = materialise_source(info)?;
    let cargo_toml = find_cargo_toml(&source)?;

    let contents = match std::fs::read_to_string(&cargo_toml) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", cargo_toml.display(), e);
            return None;
        }
    };

    let crates = parse_cargo_dependencies(&contents);
    debug!(target: LOG_TARGET, "direct dependencies: {:?}", crates);
    let (bi, nbi) = map_crates_to_nix(&crates);
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
}

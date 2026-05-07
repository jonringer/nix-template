//! Shared source materialisation utility.
//!
//! This module provides a single `materialise_source` function that fetches
//! and unpacks a source tree into the Nix store, returning the store path.
//! Used by template detection, Rust dependency inference, and Go dependency
//! inference.

use crate::types::{ExpressionInfo, Fetcher};
use log::debug;
use std::path::PathBuf;
use std::process::Command;

const LOG_TARGET: &str = "nix-template::source";

/// Materialise the source tree referenced by `info` into the Nix store
/// and return the resulting `/nix/store/...-source` path.
///
/// Supports `fetchFromGitHub`, `fetchFromGitea`, and `fetchFromGitLab`.
/// Returns `None` for fetchers we can't cleanly drive headlessly or when
/// the source hash is not yet known.
pub fn materialise_source(info: &ExpressionInfo) -> Option<PathBuf> {
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
        Fetcher::gitlab => format!(
            "(import <nixpkgs> {{}}).fetchFromGitLab {{ owner = \"{owner}\"; repo = \"{repo}\"; rev = \"{rev}\"; sha256 = \"{sha}\"; }}",
            owner = info.owner,
            repo = info.pname,
            rev = rev,
            sha = info.src_sha,
        ),
        _ => {
            debug!(
                target: LOG_TARGET,
                "fetcher {:?} not supported for source materialisation",
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
            // Provide detailed information about why the command failed
            match o.status.code() {
                Some(code) => {
                    eprintln!("Warning: nix-build exited with code {}", code);
                    debug!(
                        target: LOG_TARGET,
                        "nix-build failed with exit code {}: {}",
                        code,
                        String::from_utf8_lossy(&o.stderr)
                    );
                }
                None => {
                    eprintln!("Warning: nix-build was killed by a signal");
                    debug!(
                        target: LOG_TARGET,
                        "nix-build killed by signal: {}",
                        String::from_utf8_lossy(&o.stderr)
                    );
                }
            }
            return None;
        }
        Err(e) => {
            eprintln!("Warning: failed to invoke nix-build: {}", e);
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

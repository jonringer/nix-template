//! Inference of Elixir variants, OTP version, and native dependencies from Mix projects.
//!
//! This module reads an Elixir project's `mix.exs` and `.tool-versions` to infer:
//! 1. Whether to use mixRelease (Phoenix apps) or buildMix (libraries)
//! 2. OTP/Erlang version from .tool-versions
//! 3. Native library dependencies for packages with NIFs
//!
//! Variant detection looks for releases configuration in mix.exs.
//! Most Elixir dependencies are pure BEAM code, but some use NIFs (Native Implemented Functions).

use log::debug;
use std::collections::BTreeSet;
use std::path::Path;

use crate::templates::types::ElixirVariant;

const LOG_TARGET: &str = "nix-template::elixir_deps";

/// Detect whether an Elixir project is a Release (Phoenix app) or Library.
///
/// Checks mix.exs for releases configuration:
/// - `defp releases do` or `def releases,` indicates a Phoenix app → mixRelease
/// - Otherwise it's a library → buildMix
pub fn detect_elixir_variant(mix_exs_path: &Path) -> ElixirVariant {
    let contents = match std::fs::read_to_string(mix_exs_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read mix.exs, defaulting to Release: {}", e
            );
            return ElixirVariant::Release; // Default to Release for Phoenix apps
        }
    };

    // Check for releases configuration (Phoenix apps)
    if contents.contains("defp releases do") || contents.contains("def releases,") {
        debug!(target: LOG_TARGET, "detected releases configuration → Release variant");
        ElixirVariant::Release
    } else {
        debug!(target: LOG_TARGET, "no releases configuration found → Library variant");
        ElixirVariant::Library
    }
}

/// Infer OTP/Erlang version from .tool-versions file.
///
/// Returns the major version (e.g., "27", "26", "25") or None if not found.
/// The .tool-versions file is commonly used by asdf version manager.
pub fn infer_otp_version(project_root: &Path) -> Option<String> {
    let tool_versions_path = project_root.join(".tool-versions");
    let contents = match std::fs::read_to_string(&tool_versions_path) {
        Ok(c) => c,
        Err(_) => {
            debug!(
                target: LOG_TARGET,
                "no .tool-versions file found, will use nixpkgs default OTP"
            );
            return None;
        }
    };

    // Parse lines like: "erlang 27.0" or "erlang 26.2.5"
    for line in contents.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "erlang" {
            // Extract major version: "27.0" → "27", "26.2.5" → "26"
            if let Some(major) = parts[1].split('.').next() {
                debug!(
                    target: LOG_TARGET,
                    "detected OTP version from .tool-versions: {}", major
                );
                return Some(major.to_string());
            }
        }
    }

    debug!(
        target: LOG_TARGET,
        "no erlang version found in .tool-versions"
    );
    None
}

/// Map Mix package names to their native library dependencies.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
/// Most Elixir packages are pure BEAM, but some use NIFs.
fn lookup_mix_package(name: &str) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    match name {
        // Crypto/hashing libraries with NIFs
        "comeonin" | "argon2_elixir" | "bcrypt_elixir" => Some((vec!["libsodium"], vec![])),
        // HTML parsing libraries (use C parsers)
        "fast_html" | "floki" => Some((vec!["libxml2"], vec![])),
        // Rust NIFs (require Rust toolchain)
        "rustler" => Some((vec![], vec!["cargo", "rustc"])),
        // Image processing
        "ex_png" => Some((vec!["libpng"], vec![])),
        "mogrify" | "imagineer" => Some((vec!["imagemagick"], vec![])),
        // Database drivers (some have optional NIFs)
        "postgrex" => None, // Pure Elixir by default
        "myxql" => None,    // Pure Elixir by default
        "ecto_sql" => None, // Depends on adapter
        // Web server
        "cowboy" => None, // Pure BEAM
        // JSON libraries
        "jason" => None, // Pure Elixir (fast but no NIFs)
        // Phoenix itself
        "phoenix" | "phoenix_html" | "phoenix_live_view" => None,
        _ => None,
    }
}

/// Infer native library dependencies from mix.lock.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
pub fn infer_native_dependencies(mix_lock_path: &Path) -> (Vec<String>, Vec<String>) {
    let contents = match std::fs::read_to_string(mix_lock_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read mix.lock: {}", e
            );
            return (Vec::new(), Vec::new());
        }
    };

    let mut build_inputs = BTreeSet::new();
    let mut native_build_inputs = BTreeSet::new();

    // Parse mix.lock for package names
    // Format: "package_name": {:hex, :package_name, ...}
    // Simple regex-based extraction of package names
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('"') || line.starts_with(':') {
            // Extract package name from lines like: "bcrypt_elixir": {:hex, :bcrypt_elixir, ...}
            if let Some(pkg_name) = line
                .split(':')
                .nth(1)
                .map(|s| s.trim_matches(|c| c == '"' || c == ','))
            {
                let pkg_name = pkg_name.trim();
                if let Some((bi, nbi)) = lookup_mix_package(pkg_name) {
                    build_inputs.extend(bi.iter().map(|s| s.to_string()));
                    native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
                    debug!(
                        target: LOG_TARGET,
                        "detected native deps for {}: buildInputs={:?}, nativeBuildInputs={:?}",
                        pkg_name, bi, nbi
                    );
                }
            }
        }
    }

    (
        build_inputs.into_iter().collect(),
        native_build_inputs.into_iter().collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_variant_release() {
        // Simulate a Phoenix app with releases
        let temp_dir = tempfile::tempdir().unwrap();
        let mix_exs = temp_dir.path().join("mix.exs");
        std::fs::write(
            &mix_exs,
            r#"
defmodule MyApp.MixProject do
  def project do
    [app: :my_app]
  end

  defp releases do
    [
      my_app: []
    ]
  end
end
"#,
        )
        .unwrap();

        assert_eq!(detect_elixir_variant(&mix_exs), ElixirVariant::Release);
    }

    #[test]
    fn test_detect_variant_library() {
        // Simulate a library without releases
        let temp_dir = tempfile::tempdir().unwrap();
        let mix_exs = temp_dir.path().join("mix.exs");
        std::fs::write(
            &mix_exs,
            r#"
defmodule MyLib.MixProject do
  def project do
    [app: :my_lib]
  end
end
"#,
        )
        .unwrap();

        assert_eq!(detect_elixir_variant(&mix_exs), ElixirVariant::Library);
    }

    #[test]
    fn test_infer_otp_version() {
        let temp_dir = tempfile::tempdir().unwrap();
        let tool_versions = temp_dir.path().join(".tool-versions");
        std::fs::write(&tool_versions, "erlang 27.0\nelixir 1.16.0\n").unwrap();

        assert_eq!(infer_otp_version(temp_dir.path()), Some("27".to_string()));
    }

    #[test]
    fn test_infer_otp_version_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert_eq!(infer_otp_version(temp_dir.path()), None);
    }

    #[test]
    fn test_lookup_mix_package_bcrypt() {
        let result = lookup_mix_package("bcrypt_elixir");
        assert!(result.is_some());
        let (bi, _nbi) = result.unwrap();
        assert!(bi.contains(&"libsodium"));
    }

    #[test]
    fn test_lookup_mix_package_rustler() {
        let result = lookup_mix_package("rustler");
        assert!(result.is_some());
        let (_bi, nbi) = result.unwrap();
        assert!(nbi.contains(&"cargo"));
        assert!(nbi.contains(&"rustc"));
    }
}

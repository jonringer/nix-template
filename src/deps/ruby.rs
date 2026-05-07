//! Inference of Ruby gem → nixpkgs build inputs.
//!
//! This module reads a Ruby project's `Gemfile.lock`, walks the gem
//! specifications, and maps well-known gems with native extensions to
//! the corresponding nixpkgs `buildInputs` and `nativeBuildInputs` entries.
//!
//! The mapping is deliberately conservative: only well-known gems are mapped.
//! Users can edit the generated expression to add anything we missed.

use crate::types::ExpressionInfo;
use log::debug;
use std::collections::BTreeSet;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::ruby_deps";

/// Static mapping from a gem name to its nixpkgs (`buildInputs`,
/// `nativeBuildInputs`) requirements.
///
/// Each tuple is `(build_inputs, native_build_inputs)`. Use `&[]` for
/// "no extra entries".
fn lookup_gem(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match name {
        // XML/HTML parsing
        "nokogiri" => Some((&["libxml2", "libxslt"], &["pkg-config"])),

        // Databases
        "pg" => Some((&["postgresql"], &[])),
        "mysql2" => Some((&["libmysqlclient"], &[])),
        "sqlite3" => Some((&["sqlite"], &[])),

        // Redis
        "redis" => Some((&[], &[])), // Pure Ruby, no native deps

        // HTTP/SSL
        "eventmachine" => Some((&["openssl"], &[])),
        "puma" => Some((&["openssl"], &[])),

        // Compression
        "bzip2-ffi" | "bzip2-ruby" => Some((&["bzip2"], &[])),

        // Image processing
        "rmagick" => Some((&["imagemagick"], &["pkg-config"])),
        "mini_magick" => Some((&["imagemagick"], &[])),

        // System libraries
        "ffi" => Some((&[], &[])), // libffi is usually available by default
        "curses" => Some((&["ncurses"], &[])),

        // JSON (native extensions for performance)
        "json" | "oj" => Some((&[], &[])), // Pure C, no external deps

        // Build tools
        "bundler" => Some((&[], &[])), // No native deps

        _ => None,
    }
}

/// Parse the given `Gemfile.lock` text and return a deduplicated list of
/// every gem name in the lockfile.
///
/// The Gemfile.lock format we parse looks like:
///
/// ```text
/// GEM
///   remote: https://rubygems.org/
///   specs:
///     nokogiri (1.13.0)
///       mini_portile2 (~> 2.8.0)
///     pg (1.3.0)
/// ```
///
/// We extract gem names from the indented lines under `specs:`.
pub fn parse_gemfile_lock(gemfile_lock: &str) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut in_specs = false;

    for line in gemfile_lock.lines() {
        let trimmed = line.trim_start();

        // Look for the "specs:" section
        if trimmed == "specs:" {
            in_specs = true;
            continue;
        }

        // Exit specs section when we hit a non-indented line (except blank lines)
        if in_specs && !line.is_empty() && !line.starts_with(' ') {
            in_specs = false;
        }

        if in_specs && line.starts_with("    ") {
            // Lines with exactly 4 spaces are gem declarations
            // Format: "    gem_name (version)" or "    gem_name"
            let gem_line = line.trim();
            if let Some(name_end) = gem_line.find(|c: char| c == ' ' || c == '(') {
                let gem_name = &gem_line[..name_end];
                if !gem_name.is_empty() {
                    names.insert(gem_name.to_owned());
                }
            }
        }
    }

    names.into_iter().collect()
}

/// Given an `ExpressionInfo` with a Ruby template, attempt to infer system
/// dependencies by reading the project's `Gemfile.lock` and mapping known
/// gems to their nixpkgs equivalents.
///
/// Returns `true` if we successfully found and parsed a lockfile; `false`
/// otherwise. Even when returning `true`, the inferred lists may be empty
/// if no mappable gems were found.
/// Core inference logic that works with any source path.
/// Returns (build_inputs, native_build_inputs) if Gemfile.lock is found.
fn infer_from_source_path(source_path: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let gemfile_lock_path = source_path.join("Gemfile.lock");

    if !gemfile_lock_path.exists() {
        debug!(
            target: LOG_TARGET,
            "No Gemfile.lock at {:?}; skipping dependency inference",
            gemfile_lock_path
        );
        return None;
    }

    let lockfile_content = match std::fs::read_to_string(&gemfile_lock_path) {
        Ok(s) => s,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "Failed to read {:?}: {}; skipping dependency inference",
                gemfile_lock_path,
                e
            );
            return None;
        }
    };

    let gems = parse_gemfile_lock(&lockfile_content);
    debug!(
        target: LOG_TARGET,
        "Parsed {} gems from Gemfile.lock",
        gems.len()
    );

    let mut build_inputs: BTreeSet<String> = BTreeSet::new();
    let mut native_build_inputs: BTreeSet<String> = BTreeSet::new();

    for gem in &gems {
        if let Some((bi, nbi)) = lookup_gem(gem) {
            debug!(target: LOG_TARGET, "Mapped gem '{}' to buildInputs={:?}, nativeBuildInputs={:?}", gem, bi, nbi);
            build_inputs.extend(bi.iter().map(|s| s.to_string()));
            native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
        }
    }

    Some((
        build_inputs.into_iter().collect(),
        native_build_inputs.into_iter().collect(),
    ))
}

/// Infer Ruby gem dependencies from a local source path.
/// Used during local project initialization (--init-flake/--init-npins).
pub fn infer_ruby_dependencies_from_path(
    source_path: &Path,
) -> Option<(Vec<String>, Vec<String>)> {
    eprintln!("Scanning local Gemfile.lock for dependencies...");
    infer_from_source_path(source_path)
}

/// Infer Ruby gem dependencies from an already-materialized source in ExpressionInfo.
/// This is the original function used when inferring from remote sources.
pub fn infer_dependencies(info: &mut ExpressionInfo) -> bool {
    if let Some((build_inputs, native_build_inputs)) =
        infer_from_source_path(&info.top_level_path)
    {
        info.build_inputs.extend(build_inputs);
        info.native_build_inputs.extend(native_build_inputs);

        debug!(
            target: LOG_TARGET,
            "Inferred buildInputs={:?}, nativeBuildInputs={:?}",
            info.build_inputs,
            info.native_build_inputs
        );

        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_gemfile_lock() {
        let lockfile = r#"GEM
  remote: https://rubygems.org/
  specs:
    nokogiri (1.13.0)
      mini_portile2 (~> 2.8.0)
    pg (1.3.0)

PLATFORMS
  ruby

DEPENDENCIES
  nokogiri
  pg
"#;
        let gems = parse_gemfile_lock(lockfile);
        assert_eq!(gems, vec!["mini_portile2", "nokogiri", "pg"]);
    }

    #[test]
    fn parse_handles_empty_or_malformed() {
        assert_eq!(parse_gemfile_lock(""), Vec::<String>::new());
        assert_eq!(parse_gemfile_lock("random text"), Vec::<String>::new());
    }

    #[test]
    fn lookup_nokogiri() {
        let result = lookup_gem("nokogiri");
        assert_eq!(
            result,
            Some((
                &["libxml2", "libxslt"] as &[&str],
                &["pkg-config"] as &[&str]
            ))
        );
    }

    #[test]
    fn lookup_pg() {
        let result = lookup_gem("pg");
        assert_eq!(result, Some((&["postgresql"] as &[&str], &[] as &[&str])));
    }

    #[test]
    fn lookup_unknown_gem() {
        assert_eq!(lookup_gem("unknown-gem-12345"), None);
    }

    #[test]
    fn infer_from_lockfile_with_known_gems() {
        use crate::types::{ExpressionInfo, Fetcher, Template};
        use std::path::PathBuf;

        // Create a temporary directory with a Gemfile.lock
        let temp_dir = std::env::temp_dir().join("nix-template-ruby-test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let lockfile_path = temp_dir.join("Gemfile.lock");
        std::fs::write(
            &lockfile_path,
            r#"GEM
  remote: https://rubygems.org/
  specs:
    nokogiri (1.13.0)
    pg (1.3.0)
"#,
        )
        .unwrap();

        let mut info = ExpressionInfo {
            pname: "test".to_owned(),
            version: "1.0.0".to_owned(),
            license: "mit".to_owned(),
            maintainer: "me".to_owned(),
            fetcher: Fetcher::github,
            template: Template::ruby,
            path_to_write: PathBuf::new(),
            top_level_path: temp_dir.clone(),
            include_documentation_links: false,
            include_meta: true,
            tag_prefix: "".to_owned(),
            owner: "test".to_owned(),
            src_sha: "sha256-test".to_owned(),
            description: "test".to_owned(),
            homepage: "https://example.com".to_owned(),
            propagated_build_inputs: Vec::new(),
            cargo_hash: "".to_owned(),
            vendor_hash: "".to_owned(),
            npm_deps_hash: "".to_owned(),
            pnpm_deps_hash: "".to_owned(),
            project_file: "".to_owned(),
            domain: "".to_owned(),
            build_inputs: Vec::new(),
            native_build_inputs: Vec::new(),
            use_cargo_lock_file: false,
            cargo_lock_git_deps: Vec::new(),
            go_module_path: String::new(),
            python_format: "setuptools".to_owned(),
        };

        let success = infer_dependencies(&mut info);
        assert!(success);

        // Should have inferred deps from nokogiri and pg
        let mut expected_build = vec!["libxml2", "libxslt", "postgresql"];
        expected_build.sort();
        let mut actual_build = info.build_inputs.clone();
        actual_build.sort();
        assert_eq!(actual_build, expected_build);

        let expected_native = vec!["pkg-config"];
        assert_eq!(info.native_build_inputs, expected_native);

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

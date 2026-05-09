//! Perl dependency parsing from META.json and META.yml files.
//!
//! This module reads CPAN metadata files to extract dependency information:
//! 1. Parse META.json for module dependencies
//! 2. Parse META.yml for module dependencies
//!
//! Perl modules typically use either:
//! - ExtUtils::MakeMaker with Makefile.PL (uses buildPerlPackage)
//! - Module::Build with Build.PL (uses buildPerlModule)
//!
//! Both build systems can generate META.json/META.yml files that contain
//! module metadata and dependency information.

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::perl_deps";

/// Parse META.json for Perl module dependencies.
///
/// META.json files use JSON format with a specific schema:
/// ```json
/// {
///   "name": "My-Module",
///   "version": "1.0.0",
///   "prereqs": {
///     "runtime": {
///       "requires": {
///         "perl": "5.10.0",
///         "Module::Name": "1.0"
///       }
///     }
///   }
/// }
/// ```
///
/// Returns a list of runtime dependencies or None if parsing fails.
pub fn parse_meta_json(meta_json_path: &Path) -> Option<Vec<String>> {
    let contents = match std::fs::read_to_string(meta_json_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read META.json: {}", e
            );
            return None;
        }
    };

    // Parse JSON
    let parsed: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to parse META.json: {}", e
            );
            return None;
        }
    };

    // Extract runtime dependencies from prereqs.runtime.requires
    let mut deps = Vec::new();
    if let Some(prereqs) = parsed.get("prereqs") {
        if let Some(runtime) = prereqs.get("runtime") {
            if let Some(requires) = runtime.get("requires") {
                if let Some(obj) = requires.as_object() {
                    for (key, _value) in obj {
                        // Skip perl itself
                        if key != "perl" {
                            deps.push(key.clone());
                        }
                    }
                }
            }
        }
    }

    if deps.is_empty() {
        debug!(target: LOG_TARGET, "no dependencies found in META.json");
        None
    } else {
        debug!(target: LOG_TARGET, "found {} dependencies in META.json", deps.len());
        Some(deps)
    }
}

/// Parse META.yml for Perl module dependencies.
///
/// META.yml files use YAML format with a specific schema:
/// ```yaml
/// name: My-Module
/// version: 1.0.0
/// requires:
///   perl: 5.10.0
///   Module::Name: 1.0
/// ```
///
/// Returns a list of runtime dependencies or None if parsing fails.
/// Note: This is a simple parser that looks for the "requires:" section.
/// For full YAML parsing, we'd need a YAML library, but for now we use
/// simple text parsing.
pub fn parse_meta_yml(meta_yml_path: &Path) -> Option<Vec<String>> {
    let contents = match std::fs::read_to_string(meta_yml_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read META.yml: {}", e
            );
            return None;
        }
    };

    // Simple parsing: look for "requires:" section and extract module names
    let mut deps = Vec::new();
    let mut in_requires = false;

    for line in contents.lines() {
        let trimmed = line.trim();

        // Check if we're entering the requires section
        if trimmed == "requires:" {
            in_requires = true;
            continue;
        }

        // If we're in requires and hit another top-level key, stop
        if in_requires && !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty() {
            in_requires = false;
        }

        // Parse dependency lines (must be indented)
        if in_requires && (line.starts_with(' ') || line.starts_with('\t')) && !trimmed.is_empty() {
            // Format: "  Module::Name: version"
            // Split on ": " (colon followed by space) to avoid splitting on :: in module names
            if let Some(parts) = trimmed.split_once(": ") {
                let module_name = parts.0.trim();
                // Skip perl itself and empty lines
                if !module_name.is_empty() && module_name != "perl" {
                    debug!(target: LOG_TARGET, "found dependency in META.yml: {}", module_name);
                    deps.push(module_name.to_string());
                }
            }
        }
    }

    if deps.is_empty() {
        debug!(target: LOG_TARGET, "no dependencies found in META.yml");
        None
    } else {
        debug!(target: LOG_TARGET, "found {} dependencies in META.yml", deps.len());
        Some(deps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_meta_json() {
        let temp_dir = TempDir::new().unwrap();
        let meta_json = temp_dir.path().join("META.json");
        fs::write(
            &meta_json,
            r#"{
  "name": "My-Module",
  "version": "1.0.0",
  "prereqs": {
    "runtime": {
      "requires": {
        "perl": "5.10.0",
        "Module::Build": "0.42",
        "Test::More": "0.96"
      }
    }
  }
}"#,
        )
        .unwrap();

        let deps = parse_meta_json(&meta_json).unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"Module::Build".to_string()));
        assert!(deps.contains(&"Test::More".to_string()));
        assert!(!deps.contains(&"perl".to_string()));
    }

    #[test]
    fn test_parse_meta_yml() {
        let temp_dir = TempDir::new().unwrap();
        let meta_yml = temp_dir.path().join("META.yml");
        fs::write(
            &meta_yml,
            r#"name: My-Module
version: 1.0.0
requires:
  perl: 5.10.0
  Module::Build: 0.42
  Test::More: 0.96
"#,
        )
        .unwrap();

        let deps = parse_meta_yml(&meta_yml).unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"Module::Build".to_string()));
        assert!(deps.contains(&"Test::More".to_string()));
        assert!(!deps.contains(&"perl".to_string()));
    }

    #[test]
    fn test_parse_meta_json_no_deps() {
        let temp_dir = TempDir::new().unwrap();
        let meta_json = temp_dir.path().join("META.json");
        fs::write(
            &meta_json,
            r#"{
  "name": "My-Module",
  "version": "1.0.0"
}"#,
        )
        .unwrap();

        let deps = parse_meta_json(&meta_json);
        assert_eq!(deps, None);
    }

    #[test]
    fn test_parse_meta_yml_no_deps() {
        let temp_dir = TempDir::new().unwrap();
        let meta_yml = temp_dir.path().join("META.yml");
        fs::write(
            &meta_yml,
            r#"name: My-Module
version: 1.0.0
"#,
        )
        .unwrap();

        let deps = parse_meta_yml(&meta_yml);
        assert_eq!(deps, None);
    }
}

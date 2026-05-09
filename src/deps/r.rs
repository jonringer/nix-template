//! R DESCRIPTION file parsing.
//!
//! This module reads R DESCRIPTION files to:
//! 1. Extract package name and version
//! 2. Parse R version requirements from Depends field
//! 3. Extract R package dependencies from Depends, Imports, and LinkingTo fields
//!
//! DESCRIPTION files use Debian Control File (DCF) format with fields like:
//! ```
//! Package: mypackage
//! Version: 1.0.0
//! Depends: R (>= 4.0.0), methods, stats
//! Imports: ggplot2, dplyr (>= 1.0.0)
//! LinkingTo: Rcpp, RcppArmadillo
//! ```
//!
//! Multi-line values are indented with spaces.

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::r_deps";

/// Parse package name from DESCRIPTION file.
///
/// Returns the package name if found, or None if the file cannot be parsed.
pub fn parse_package_name(description_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(description_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read DESCRIPTION: {}", e
            );
            return None;
        }
    };

    parse_dcf_field(&contents, "Package")
}

/// Infer R version from DESCRIPTION Depends field.
///
/// Parses the Depends field for R version requirements like:
/// ```
/// Depends: R (>= 4.0.0)
/// ```
///
/// Returns the R version string (e.g., "4.0.0") if found, None otherwise.
pub fn infer_r_version(description_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(description_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read DESCRIPTION: {}", e
            );
            return None;
        }
    };

    let depends = parse_dcf_field(&contents, "Depends")?;

    // Look for R version: "R (>= 4.0.0)" or "R (>= 3.5.0)"
    for dep in depends.split(',') {
        let dep = dep.trim();
        if dep.starts_with("R (") || dep.starts_with("R(") {
            // Extract version from "R (>= 4.0.0)"
            if let Some(start) = dep.find(">=") {
                let version_part = &dep[start + 2..].trim();
                // Extract version until closing paren or comma
                let version = version_part
                    .split(&[')', ','][..])
                    .next()
                    .map(|s| s.trim().to_string())?;

                debug!(target: LOG_TARGET, "detected R version: {}", version);
                return Some(version);
            }
        }
    }

    debug!(target: LOG_TARGET, "no R version found in Depends field");
    None
}

/// Parse R package dependencies from DESCRIPTION file.
///
/// Extracts package names from Depends, Imports, and LinkingTo fields.
/// Filters out "R" itself and base packages.
///
/// Returns a vector of package names.
pub fn parse_r_dependencies(description_path: &Path) -> Vec<String> {
    let contents = match std::fs::read_to_string(description_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read DESCRIPTION: {}", e
            );
            return Vec::new();
        }
    };

    let mut deps = Vec::new();

    // Parse Depends field
    if let Some(depends) = parse_dcf_field(&contents, "Depends") {
        deps.extend(extract_package_names(&depends));
    }

    // Parse Imports field
    if let Some(imports) = parse_dcf_field(&contents, "Imports") {
        deps.extend(extract_package_names(&imports));
    }

    // Parse LinkingTo field
    if let Some(linking_to) = parse_dcf_field(&contents, "LinkingTo") {
        deps.extend(extract_package_names(&linking_to));
    }

    // Filter out "R" itself and common base packages
    let base_packages = ["R", "methods", "stats", "utils", "graphics", "grDevices", "datasets", "base"];
    deps.retain(|pkg| !base_packages.contains(&pkg.as_str()));

    debug!(target: LOG_TARGET, "parsed {} R package dependencies", deps.len());
    deps
}

/// Parse a single field from DCF format.
///
/// Handles multi-line fields where continuation lines start with whitespace.
fn parse_dcf_field(contents: &str, field_name: &str) -> Option<String> {
    let mut lines = contents.lines();
    let mut field_value = String::new();
    let mut in_field = false;

    while let Some(line) = lines.next() {
        if line.starts_with(field_name) && line.contains(':') {
            // Found the field, extract value after colon
            if let Some(colon_pos) = line.find(':') {
                field_value = line[colon_pos + 1..].trim().to_string();
                in_field = true;
            }
        } else if in_field {
            // Check if this is a continuation line (starts with whitespace)
            if line.starts_with(' ') || line.starts_with('\t') {
                field_value.push(' ');
                field_value.push_str(line.trim());
            } else {
                // End of field
                break;
            }
        }
    }

    if field_value.is_empty() {
        None
    } else {
        Some(field_value)
    }
}

/// Extract package names from a comma-separated dependency list.
///
/// Input: "ggplot2, dplyr (>= 1.0.0), R (>= 4.0.0)"
/// Output: ["ggplot2", "dplyr"]
fn extract_package_names(dep_string: &str) -> Vec<String> {
    dep_string
        .split(',')
        .filter_map(|dep| {
            let dep = dep.trim();
            // Extract package name (before any parentheses or version spec)
            let pkg_name = dep
                .split(&['(', ' '][..])
                .next()
                .map(|s| s.trim())?;

            if pkg_name.is_empty() {
                None
            } else {
                Some(pkg_name.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_package_name() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Version: 1.0.0
"#,
        )
        .unwrap();

        let name = parse_package_name(&desc_path);
        assert_eq!(name, Some("mypackage".to_string()));
    }

    #[test]
    fn test_infer_r_version() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Depends: R (>= 4.0.0), methods
"#,
        )
        .unwrap();

        let version = infer_r_version(&desc_path);
        assert_eq!(version, Some("4.0.0".to_string()));
    }

    #[test]
    fn test_infer_r_version_no_space() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Depends: R(>= 3.5.0)
"#,
        )
        .unwrap();

        let version = infer_r_version(&desc_path);
        assert_eq!(version, Some("3.5.0".to_string()));
    }

    #[test]
    fn test_parse_dependencies_simple() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Imports: ggplot2, dplyr
"#,
        )
        .unwrap();

        let deps = parse_r_dependencies(&desc_path);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"ggplot2".to_string()));
        assert!(deps.contains(&"dplyr".to_string()));
    }

    #[test]
    fn test_parse_dependencies_with_versions() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Depends: R (>= 4.0.0)
Imports: ggplot2 (>= 3.0.0), dplyr (>= 1.0.0)
LinkingTo: Rcpp
"#,
        )
        .unwrap();

        let deps = parse_r_dependencies(&desc_path);
        // Should have ggplot2, dplyr, Rcpp (R is filtered out)
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&"ggplot2".to_string()));
        assert!(deps.contains(&"dplyr".to_string()));
        assert!(deps.contains(&"Rcpp".to_string()));
        assert!(!deps.contains(&"R".to_string()));
    }

    #[test]
    fn test_parse_dependencies_multiline() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Imports: ggplot2,
    dplyr,
    tidyr
"#,
        )
        .unwrap();

        let deps = parse_r_dependencies(&desc_path);
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&"ggplot2".to_string()));
        assert!(deps.contains(&"dplyr".to_string()));
        assert!(deps.contains(&"tidyr".to_string()));
    }

    #[test]
    fn test_filter_base_packages() {
        let temp_dir = TempDir::new().unwrap();
        let desc_path = temp_dir.path().join("DESCRIPTION");
        fs::write(
            &desc_path,
            r#"Package: mypackage
Depends: R (>= 4.0.0), methods, stats, utils
Imports: ggplot2
"#,
        )
        .unwrap();

        let deps = parse_r_dependencies(&desc_path);
        // Should only have ggplot2, base packages filtered out
        assert_eq!(deps.len(), 1);
        assert!(deps.contains(&"ggplot2".to_string()));
        assert!(!deps.contains(&"methods".to_string()));
        assert!(!deps.contains(&"stats".to_string()));
        assert!(!deps.contains(&"utils".to_string()));
    }

    #[test]
    fn test_parse_dcf_field() {
        let contents = r#"Package: mypackage
Version: 1.0.0
Description: This is a package
    with a multi-line
    description.
Author: John Doe
"#;

        let package = parse_dcf_field(contents, "Package");
        assert_eq!(package, Some("mypackage".to_string()));

        let desc = parse_dcf_field(contents, "Description");
        assert_eq!(
            desc,
            Some("This is a package with a multi-line description.".to_string())
        );

        let author = parse_dcf_field(contents, "Author");
        assert_eq!(author, Some("John Doe".to_string()));

        let missing = parse_dcf_field(contents, "License");
        assert_eq!(missing, None);
    }
}

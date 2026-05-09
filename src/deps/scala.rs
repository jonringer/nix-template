//! Inference of Scala/SBT version information from build files.
//!
//! This module reads Scala/SBT project files to:
//! 1. Extract Scala version from build.sbt
//! 2. Extract SBT version from project/build.properties
//!
//! Scala projects typically use SBT (Scala Build Tool) for building and dependency management.
//! The Scala version is defined in build.sbt, and the SBT version is defined in project/build.properties.

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::scala_deps";

/// Extract Scala version from build.sbt file.
///
/// build.sbt files use Scala syntax to define build configuration:
/// ```scala
/// scalaVersion := "2.13.12"
/// ```
///
/// Returns the Scala version or None if not found.
pub fn extract_scala_version(build_sbt_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(build_sbt_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read build.sbt: {}", e
            );
            return None;
        }
    };

    // Parse for scalaVersion := "X.Y.Z"
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("scalaVersion") {
            // Extract version from scalaVersion := "2.13.12"
            if let Some(version_start) = trimmed.find('"') {
                if let Some(version_end) = trimmed.rfind('"') {
                    if version_end > version_start {
                        let version = &trimmed[version_start + 1..version_end];
                        debug!(target: LOG_TARGET, "extracted Scala version: {}", version);
                        return Some(version.to_string());
                    }
                }
            }
        }
    }

    debug!(target: LOG_TARGET, "no Scala version found in build.sbt");
    None
}

/// Extract SBT version from project/build.properties file.
///
/// build.properties files use Java properties format:
/// ```properties
/// sbt.version=1.9.7
/// ```
///
/// Returns the SBT version or None if not found.
pub fn extract_sbt_version(build_properties_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(build_properties_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read build.properties: {}", e
            );
            return None;
        }
    };

    // Parse for sbt.version=X.Y.Z
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("sbt.version") {
            // Extract version from sbt.version=1.9.7
            if let Some(equals_pos) = trimmed.find('=') {
                let version = trimmed[equals_pos + 1..].trim();
                debug!(target: LOG_TARGET, "extracted SBT version: {}", version);
                return Some(version.to_string());
            }
        }
    }

    debug!(target: LOG_TARGET, "no SBT version found in build.properties");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_scala_version() {
        let temp_dir = TempDir::new().unwrap();
        let build_sbt = temp_dir.path().join("build.sbt");
        fs::write(
            &build_sbt,
            r#"
name := "my-scala-project"
version := "1.0.0"
scalaVersion := "2.13.12"
"#,
        )
        .unwrap();

        let version = extract_scala_version(&build_sbt);
        assert_eq!(version, Some("2.13.12".to_string()));
    }

    #[test]
    fn test_extract_scala_version_with_spaces() {
        let temp_dir = TempDir::new().unwrap();
        let build_sbt = temp_dir.path().join("build.sbt");
        fs::write(
            &build_sbt,
            r#"
scalaVersion  :=  "3.3.1"
"#,
        )
        .unwrap();

        let version = extract_scala_version(&build_sbt);
        assert_eq!(version, Some("3.3.1".to_string()));
    }

    #[test]
    fn test_extract_sbt_version() {
        let temp_dir = TempDir::new().unwrap();
        let build_properties = temp_dir.path().join("build.properties");
        fs::write(
            &build_properties,
            r#"
# SBT version
sbt.version=1.9.7
"#,
        )
        .unwrap();

        let version = extract_sbt_version(&build_properties);
        assert_eq!(version, Some("1.9.7".to_string()));
    }

    #[test]
    fn test_extract_sbt_version_no_spaces() {
        let temp_dir = TempDir::new().unwrap();
        let build_properties = temp_dir.path().join("build.properties");
        fs::write(&build_properties, "sbt.version=1.8.0").unwrap();

        let version = extract_sbt_version(&build_properties);
        assert_eq!(version, Some("1.8.0".to_string()));
    }

    #[test]
    fn test_no_scala_version_found() {
        let temp_dir = TempDir::new().unwrap();
        let build_sbt = temp_dir.path().join("build.sbt");
        fs::write(
            &build_sbt,
            r#"
name := "my-project"
version := "1.0.0"
"#,
        )
        .unwrap();

        let version = extract_scala_version(&build_sbt);
        assert_eq!(version, None);
    }

    #[test]
    fn test_no_sbt_version_found() {
        let temp_dir = TempDir::new().unwrap();
        let build_properties = temp_dir.path().join("build.properties");
        fs::write(&build_properties, "# Empty file").unwrap();

        let version = extract_sbt_version(&build_properties);
        assert_eq!(version, None);
    }
}

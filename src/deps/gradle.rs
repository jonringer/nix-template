//! Inference of Gradle variants, JDK version, and DSL from Gradle projects.
//!
//! This module reads a Gradle project's build files and properties to infer:
//! 1. Whether to use gradle2nix (gradle-deps.json) or manual dependency management
//! 2. Build DSL: Groovy (.gradle) or Kotlin (.gradle.kts)
//! 3. JDK version from gradle.properties or build.gradle*
//!
//! Gradle projects can be packaged two ways:
//! - With gradle2nix: generates gradle-deps.json for reproducible dependency fetching
//! - Manual: uses gradle.fetchDeps with FOD hash (simpler but less precise)

use log::debug;
use std::path::Path;

use crate::templates::types::{GradleDsl, GradleVariant};

const LOG_TARGET: &str = "nix-template::gradle_deps";

/// Detect whether to use gradle2nix or manual dependency management.
///
/// If gradle-deps.json exists, use Gradle2nix variant.
/// Otherwise, default to Manual variant with gradle.fetchDeps.
pub fn detect_gradle_variant(project_root: &Path) -> GradleVariant {
    if project_root.join("gradle-deps.json").exists() {
        debug!(target: LOG_TARGET, "found gradle-deps.json → Gradle2nix variant");
        GradleVariant::Gradle2nix
    } else {
        debug!(target: LOG_TARGET, "no gradle-deps.json → Manual variant");
        GradleVariant::Manual
    }
}

/// Detect the Gradle build DSL (Groovy or Kotlin).
///
/// Checks for build.gradle.kts (Kotlin) vs build.gradle (Groovy).
/// Kotlin DSL takes precedence if both exist (as it's the modern default).
pub fn detect_gradle_dsl(project_root: &Path) -> GradleDsl {
    if project_root.join("build.gradle.kts").exists() {
        debug!(target: LOG_TARGET, "found build.gradle.kts → Kotlin DSL");
        GradleDsl::Kotlin
    } else if project_root.join("build.gradle").exists() {
        debug!(target: LOG_TARGET, "found build.gradle → Groovy DSL");
        GradleDsl::Groovy
    } else {
        // Fallback to Groovy if neither exists (shouldn't happen in valid projects)
        debug!(target: LOG_TARGET, "no build file found, defaulting to Groovy DSL");
        GradleDsl::Groovy
    }
}

/// Infer JDK version from gradle.properties or build.gradle*.
///
/// Checks in order:
/// 1. gradle.properties: `javaVersion=17` or `java.version=17`
/// 2. build.gradle*: sourceCompatibility or targetCompatibility
/// 3. Defaults to jdk17 (current LTS)
pub fn infer_gradle_jdk_version(project_root: &Path) -> String {
    // First try gradle.properties
    if let Some(version) = read_gradle_properties(project_root) {
        debug!(target: LOG_TARGET, "JDK version from gradle.properties: {}", version);
        return version;
    }

    // Then try build.gradle.kts (Kotlin DSL)
    if let Some(version) = read_build_gradle_kts(project_root) {
        debug!(target: LOG_TARGET, "JDK version from build.gradle.kts: {}", version);
        return version;
    }

    // Finally try build.gradle (Groovy DSL)
    if let Some(version) = read_build_gradle(project_root) {
        debug!(target: LOG_TARGET, "JDK version from build.gradle: {}", version);
        return version;
    }

    // Default to jdk17 (current LTS)
    debug!(target: LOG_TARGET, "no JDK version found, defaulting to jdk17");
    "jdk17".to_owned()
}

/// Read JDK version from gradle.properties.
///
/// Looks for keys like:
/// - javaVersion=17
/// - java.version=17
/// - sourceCompatibility=17
fn read_gradle_properties(project_root: &Path) -> Option<String> {
    let props_path = project_root.join("gradle.properties");
    let contents = match std::fs::read_to_string(&props_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    for line in contents.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        // Parse key=value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "javaVersion" | "java.version" | "sourceCompatibility" | "targetCompatibility" => {
                    // Extract version number (e.g., "17" or "1.8" → "8")
                    return Some(normalize_jdk_version(value));
                }
                _ => {}
            }
        }
    }

    None
}

/// Read JDK version from build.gradle.kts (Kotlin DSL).
///
/// Looks for patterns like:
/// - java.sourceCompatibility = JavaVersion.VERSION_17
/// - sourceCompatibility = JavaVersion.VERSION_17
/// - java.toolchain.languageVersion.set(JavaLanguageVersion.of(17))
fn read_build_gradle_kts(project_root: &Path) -> Option<String> {
    let build_path = project_root.join("build.gradle.kts");
    let contents = match std::fs::read_to_string(&build_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Look for VERSION_XX patterns
    if let Some(version) = extract_version_from_pattern(&contents, "VERSION_") {
        return Some(format!("jdk{}", version));
    }

    // Look for JavaLanguageVersion.of(XX)
    if let Some(version) = extract_version_from_pattern(&contents, "JavaLanguageVersion.of(") {
        return Some(format!("jdk{}", version));
    }

    // Look for sourceCompatibility = "XX" or 'XX'
    if let Some(version) = extract_version_from_assignment(&contents, "sourceCompatibility") {
        return Some(normalize_jdk_version(&version));
    }

    if let Some(version) = extract_version_from_assignment(&contents, "targetCompatibility") {
        return Some(normalize_jdk_version(&version));
    }

    None
}

/// Read JDK version from build.gradle (Groovy DSL).
///
/// Looks for patterns like:
/// - sourceCompatibility = '17'
/// - sourceCompatibility = JavaVersion.VERSION_17
/// - java { sourceCompatibility = JavaVersion.VERSION_17 }
fn read_build_gradle(project_root: &Path) -> Option<String> {
    let build_path = project_root.join("build.gradle");
    let contents = match std::fs::read_to_string(&build_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Look for VERSION_XX patterns
    if let Some(version) = extract_version_from_pattern(&contents, "VERSION_") {
        return Some(format!("jdk{}", version));
    }

    // Look for sourceCompatibility = 'XX' or "XX"
    if let Some(version) = extract_version_from_assignment(&contents, "sourceCompatibility") {
        return Some(normalize_jdk_version(&version));
    }

    if let Some(version) = extract_version_from_assignment(&contents, "targetCompatibility") {
        return Some(normalize_jdk_version(&version));
    }

    None
}

/// Extract version number from patterns like "VERSION_17" or "JavaLanguageVersion.of(17)".
fn extract_version_from_pattern(contents: &str, pattern: &str) -> Option<String> {
    for line in contents.lines() {
        if let Some(pos) = line.find(pattern) {
            let after = &line[pos + pattern.len()..];
            // Extract digits
            let version: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !version.is_empty() {
                return Some(version);
            }
        }
    }
    None
}

/// Extract version from assignment like "sourceCompatibility = '17'" or "sourceCompatibility = \"17\"".
fn extract_version_from_assignment(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        let line = line.trim();
        if line.contains(key) && line.contains('=') {
            // Split on = and extract the value part
            if let Some((_, value)) = line.split_once('=') {
                let value = value.trim();
                // Remove quotes and extract digits/dots
                let version: String = value
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if !version.is_empty() {
                    return Some(version);
                }
            }
        }
    }
    None
}

/// Normalize JDK version to nixpkgs format.
///
/// Examples:
/// - "17" → "jdk17"
/// - "1.8" → "jdk8"
/// - "11" → "jdk11"
fn normalize_jdk_version(version: &str) -> String {
    // Handle legacy versions like "1.8" → "8"
    let normalized = if version.starts_with("1.") {
        version.strip_prefix("1.").unwrap_or(version)
    } else {
        version
    };

    // Extract just the major version (e.g., "17.0.2" → "17")
    let major = normalized.split('.').next().unwrap_or(normalized);

    format!("jdk{}", major)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_variant_gradle2nix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let gradle_deps = temp_dir.path().join("gradle-deps.json");
        std::fs::write(&gradle_deps, "{}").unwrap();

        assert_eq!(
            detect_gradle_variant(temp_dir.path()),
            GradleVariant::Gradle2nix
        );
    }

    #[test]
    fn test_detect_variant_manual() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_gradle_variant(temp_dir.path()),
            GradleVariant::Manual
        );
    }

    #[test]
    fn test_detect_dsl_kotlin() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_file = temp_dir.path().join("build.gradle.kts");
        std::fs::write(&build_file, "").unwrap();

        assert_eq!(detect_gradle_dsl(temp_dir.path()), GradleDsl::Kotlin);
    }

    #[test]
    fn test_detect_dsl_groovy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_file = temp_dir.path().join("build.gradle");
        std::fs::write(&build_file, "").unwrap();

        assert_eq!(detect_gradle_dsl(temp_dir.path()), GradleDsl::Groovy);
    }

    #[test]
    fn test_detect_dsl_kotlin_precedence() {
        // If both exist, Kotlin takes precedence
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("build.gradle.kts"), "").unwrap();
        std::fs::write(temp_dir.path().join("build.gradle"), "").unwrap();

        assert_eq!(detect_gradle_dsl(temp_dir.path()), GradleDsl::Kotlin);
    }

    #[test]
    fn test_infer_jdk_from_gradle_properties() {
        let temp_dir = tempfile::tempdir().unwrap();
        let props = temp_dir.path().join("gradle.properties");
        std::fs::write(&props, "javaVersion=17\n").unwrap();

        assert_eq!(infer_gradle_jdk_version(temp_dir.path()), "jdk17");
    }

    #[test]
    fn test_infer_jdk_from_build_gradle_kts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build = temp_dir.path().join("build.gradle.kts");
        std::fs::write(
            &build,
            r#"
java {
    sourceCompatibility = JavaVersion.VERSION_21
}
"#,
        )
        .unwrap();

        assert_eq!(infer_gradle_jdk_version(temp_dir.path()), "jdk21");
    }

    #[test]
    fn test_infer_jdk_from_build_gradle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build = temp_dir.path().join("build.gradle");
        std::fs::write(
            &build,
            r#"
sourceCompatibility = '11'
targetCompatibility = '11'
"#,
        )
        .unwrap();

        assert_eq!(infer_gradle_jdk_version(temp_dir.path()), "jdk11");
    }

    #[test]
    fn test_infer_jdk_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert_eq!(infer_gradle_jdk_version(temp_dir.path()), "jdk17");
    }

    #[test]
    fn test_normalize_jdk_version_legacy() {
        assert_eq!(normalize_jdk_version("1.8"), "jdk8");
        assert_eq!(normalize_jdk_version("1.7"), "jdk7");
    }

    #[test]
    fn test_normalize_jdk_version_modern() {
        assert_eq!(normalize_jdk_version("17"), "jdk17");
        assert_eq!(normalize_jdk_version("21"), "jdk21");
        assert_eq!(normalize_jdk_version("11"), "jdk11");
    }

    #[test]
    fn test_normalize_jdk_version_with_patch() {
        assert_eq!(normalize_jdk_version("17.0.2"), "jdk17");
        assert_eq!(normalize_jdk_version("11.0.15"), "jdk11");
    }
}

//! Inference of JDK version and native dependencies from Maven `pom.xml`.
//!
//! This module reads a Maven project's `pom.xml` and extracts:
//! 1. JDK version from compiler configuration properties
//! 2. Native library dependencies for Maven artifacts (e.g., JDBC drivers)
//!
//! JDK version is inferred from Maven compiler plugin properties.
//! Most Java dependencies are pure JVM and don't require native libraries,
//! but some (like JDBC drivers) may need system libraries for optimal performance.
//!
//! As with other dependency modules, this is best-effort: users can edit
//! the generated expression to add anything we missed.

use log::debug;
use std::collections::BTreeSet;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::maven_deps";

/// Map Maven artifact coordinates to their native library dependencies.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
/// Most Maven artifacts don't require native libs (pure JVM), but some do.
fn lookup_maven_artifact(
    group_id: &str,
    artifact_id: &str,
) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    let key = format!("{}:{}", group_id, artifact_id);
    match key.as_str() {
        // JDBC drivers - may benefit from native database client libraries
        "org.postgresql:postgresql" => Some((vec!["postgresql"], vec![])),
        "mysql:mysql-connector-java" | "com.mysql:mysql-connector-j" => {
            Some((vec!["mysql"], vec![]))
        }
        "org.xerial:sqlite-jdbc" => Some((vec!["sqlite"], vec![])),
        // JavaFX requires native graphics libraries
        "org.openjfx:javafx-base"
        | "org.openjfx:javafx-controls"
        | "org.openjfx:javafx-graphics" => Some((vec!["openjfx"], vec![])),
        _ => None,
    }
}

/// Parse `pom.xml` and infer the JDK version from Maven compiler properties.
///
/// Looks for:
/// - `<maven.compiler.source>21</maven.compiler.source>`
/// - `<maven.compiler.target>21</maven.compiler.target>`
/// - `<java.version>17</java.version>` (common in Spring Boot projects)
///
/// Returns a JDK package name like "jdk21", "jdk17", etc.
/// Defaults to "jdk21" (current LTS) if no version is specified.
pub fn infer_jdk_version(pom_xml_path: &Path) -> String {
    let contents = match std::fs::read_to_string(pom_xml_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read pom.xml: {}", e
            );
            return "jdk21".to_string(); // Default to latest LTS
        }
    };

    // Try to extract version from properties
    // Pattern: <maven.compiler.source>21</maven.compiler.source>
    let patterns = [
        r"<maven\.compiler\.source>(\d+)</maven\.compiler\.source>",
        r"<maven\.compiler\.target>(\d+)</maven\.compiler\.target>",
        r"<java\.version>(\d+)(?:\.\d+)?</java\.version>",
    ];

    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(&contents) {
                if let Some(version) = captures.get(1) {
                    let version_str = version.as_str();
                    // Map to JDK package name: "21" -> "jdk21", "17" -> "jdk17"
                    let jdk_package = format!("jdk{}", version_str);
                    debug!(
                        target: LOG_TARGET,
                        "detected JDK version from {}: {}", pattern, jdk_package
                    );
                    return jdk_package;
                }
            }
        }
    }

    debug!(
        target: LOG_TARGET,
        "no JDK version found in pom.xml, defaulting to jdk21"
    );
    "jdk21".to_string()
}

/// Infer native library dependencies from pom.xml Maven dependencies.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
pub fn infer_native_dependencies(pom_xml_path: &Path) -> (Vec<String>, Vec<String>) {
    let contents = match std::fs::read_to_string(pom_xml_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read pom.xml: {}", e
            );
            return (Vec::new(), Vec::new());
        }
    };

    let mut build_inputs = BTreeSet::new();
    let mut native_build_inputs = BTreeSet::new();

    // Parse dependency blocks: <groupId>...</groupId><artifactId>...</artifactId>
    // This is a simple regex-based parser - not a full XML parser
    let dep_pattern = r#"<groupId>([\w\.\-]+)</groupId>\s*<artifactId>([\w\.\-]+)</artifactId>"#;

    if let Ok(re) = regex::Regex::new(dep_pattern) {
        for captures in re.captures_iter(&contents) {
            if let (Some(group_id), Some(artifact_id)) = (captures.get(1), captures.get(2)) {
                let group = group_id.as_str();
                let artifact = artifact_id.as_str();

                if let Some((bi, nbi)) = lookup_maven_artifact(group, artifact) {
                    build_inputs.extend(bi.iter().map(|s| s.to_string()));
                    native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
                    debug!(
                        target: LOG_TARGET,
                        "detected native deps for {}:{}: buildInputs={:?}, nativeBuildInputs={:?}",
                        group, artifact, bi, nbi
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
    fn test_lookup_maven_artifact() {
        let result = lookup_maven_artifact("org.postgresql", "postgresql");
        assert!(result.is_some());
        let (bi, _nbi) = result.unwrap();
        assert!(bi.contains(&"postgresql"));
    }

    #[test]
    fn test_lookup_maven_artifact_mysql() {
        let result = lookup_maven_artifact("mysql", "mysql-connector-java");
        assert!(result.is_some());
        let (bi, _nbi) = result.unwrap();
        assert!(bi.contains(&"mysql"));
    }

    #[test]
    fn test_default_jdk_version() {
        // If file doesn't exist, should default to jdk21
        let version = infer_jdk_version(Path::new("/nonexistent/pom.xml"));
        assert_eq!(version, "jdk21");
    }
}

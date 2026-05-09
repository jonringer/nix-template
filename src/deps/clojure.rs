//! Inference of Clojure JDK version information from build files.
//!
//! This module reads Clojure project files to:
//! 1. Infer JDK version from deps.edn (Clojure CLI tools)
//! 2. Infer JDK version from project.clj (Leiningen)
//!
//! Clojure projects can use either:
//! - Clojure CLI tools with deps.edn for dependency management
//! - Leiningen with project.clj for builds and dependencies
//!
//! JDK version inference is best-effort; if no explicit version is found,
//! we default to None (nixpkgs will use its default JDK).

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::clojure_deps";

/// Infer JDK version from deps.edn file.
///
/// deps.edn files use EDN (Extensible Data Notation) format.
/// We look for common patterns like:
/// - :java-cmd with version references
/// - Comments indicating Java version requirements
///
/// Returns the JDK version or None if not found.
/// Since deps.edn doesn't typically specify Java versions explicitly,
/// this is a best-effort heuristic.
pub fn infer_jdk_version_from_deps(deps_edn_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(deps_edn_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read deps.edn: {}", e
            );
            return None;
        }
    };

    // Look for Java version comments or directives
    // Example: ;; Requires Java 17 or later
    for line in contents.lines() {
        let trimmed = line.trim().to_lowercase();

        // Check for comment patterns mentioning Java versions
        if trimmed.contains("java") && (trimmed.contains("require") || trimmed.contains("need")) {
            // Try to extract version number
            if trimmed.contains("21") {
                debug!(target: LOG_TARGET, "inferred JDK 21 from deps.edn comment");
                return Some("21".to_string());
            } else if trimmed.contains("17") {
                debug!(target: LOG_TARGET, "inferred JDK 17 from deps.edn comment");
                return Some("17".to_string());
            } else if trimmed.contains("11") {
                debug!(target: LOG_TARGET, "inferred JDK 11 from deps.edn comment");
                return Some("11".to_string());
            }
        }
    }

    debug!(target: LOG_TARGET, "no JDK version found in deps.edn, using nixpkgs default");
    None
}

/// Infer JDK version from project.clj file (Leiningen).
///
/// project.clj files use Clojure syntax for configuration:
/// ```clojure
/// (defproject myproject "0.1.0"
///   :dependencies [[org.clojure/clojure "1.11.1"]]
///   :java-source-paths ["src/java"]
///   :javac-options ["-target" "17" "-source" "17"])
/// ```
///
/// Returns the JDK version or None if not found.
pub fn infer_jdk_version_from_project(project_clj_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(project_clj_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read project.clj: {}", e
            );
            return None;
        }
    };

    // Look for :javac-options with -target or -source
    // Example: :javac-options ["-target" "17" "-source" "17"]
    if let Some(javac_start) = contents.find(":javac-options") {
        let after_javac = &contents[javac_start..];

        // Look for -target or -source followed by version
        if let Some(target_pos) = after_javac.find("\"-target\"") {
            let after_target = &after_javac[target_pos + 9..]; // Skip '"-target"'
            // Find next quoted string, skipping whitespace
            let trimmed = after_target.trim_start();
            if let Some(quote_start) = trimmed.find('"') {
                if let Some(quote_end) = trimmed[quote_start + 1..].find('"') {
                    let version = &trimmed[quote_start + 1..quote_start + 1 + quote_end];
                    debug!(target: LOG_TARGET, "inferred JDK {} from project.clj :javac-options", version);
                    return Some(version.to_string());
                }
            }
        }
    }

    // Look for comment patterns similar to deps.edn
    for line in contents.lines() {
        let trimmed = line.trim().to_lowercase();

        if trimmed.starts_with(";") && trimmed.contains("java")
            && (trimmed.contains("require") || trimmed.contains("need")) {
            if trimmed.contains("21") {
                debug!(target: LOG_TARGET, "inferred JDK 21 from project.clj comment");
                return Some("21".to_string());
            } else if trimmed.contains("17") {
                debug!(target: LOG_TARGET, "inferred JDK 17 from project.clj comment");
                return Some("17".to_string());
            } else if trimmed.contains("11") {
                debug!(target: LOG_TARGET, "inferred JDK 11 from project.clj comment");
                return Some("11".to_string());
            }
        }
    }

    debug!(target: LOG_TARGET, "no JDK version found in project.clj, using nixpkgs default");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_infer_jdk_from_deps_edn_comment() {
        let temp_dir = TempDir::new().unwrap();
        let deps_edn = temp_dir.path().join("deps.edn");
        fs::write(
            &deps_edn,
            r#"
;; Requires Java 17 or later
{:deps {org.clojure/clojure {:mvn/version "1.11.1"}}}
"#,
        )
        .unwrap();

        let version = infer_jdk_version_from_deps(&deps_edn);
        assert_eq!(version, Some("17".to_string()));
    }

    #[test]
    fn test_infer_jdk_from_deps_edn_no_version() {
        let temp_dir = TempDir::new().unwrap();
        let deps_edn = temp_dir.path().join("deps.edn");
        fs::write(
            &deps_edn,
            r#"
{:deps {org.clojure/clojure {:mvn/version "1.11.1"}}}
"#,
        )
        .unwrap();

        let version = infer_jdk_version_from_deps(&deps_edn);
        assert_eq!(version, None);
    }

    #[test]
    fn test_infer_jdk_from_project_clj_javac_options() {
        let temp_dir = TempDir::new().unwrap();
        let project_clj = temp_dir.path().join("project.clj");
        fs::write(
            &project_clj,
            r#"
(defproject myproject "0.1.0"
  :dependencies [[org.clojure/clojure "1.11.1"]]
  :javac-options ["-target" "17" "-source" "17"])
"#,
        )
        .unwrap();

        let version = infer_jdk_version_from_project(&project_clj);
        assert_eq!(version, Some("17".to_string()));
    }

    #[test]
    fn test_infer_jdk_from_project_clj_comment() {
        let temp_dir = TempDir::new().unwrap();
        let project_clj = temp_dir.path().join("project.clj");
        fs::write(
            &project_clj,
            r#"
;; Requires Java 21
(defproject myproject "0.1.0"
  :dependencies [[org.clojure/clojure "1.11.1"]])
"#,
        )
        .unwrap();

        let version = infer_jdk_version_from_project(&project_clj);
        assert_eq!(version, Some("21".to_string()));
    }

    #[test]
    fn test_no_jdk_version_found_in_project_clj() {
        let temp_dir = TempDir::new().unwrap();
        let project_clj = temp_dir.path().join("project.clj");
        fs::write(
            &project_clj,
            r#"
(defproject myproject "0.1.0"
  :dependencies [[org.clojure/clojure "1.11.1"]])
"#,
        )
        .unwrap();

        let version = infer_jdk_version_from_project(&project_clj);
        assert_eq!(version, None);
    }
}

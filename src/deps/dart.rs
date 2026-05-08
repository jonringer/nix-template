//! Inference of Dart executables and Flutter exclusion from Dart projects.
//!
//! This module reads a Dart project's `pubspec.yaml` to:
//! 1. Parse executable names from the executables section
//! 2. Detect and exclude Flutter projects (not supported by buildDartApplication)
//! 3. Extract Dart SDK version constraints (reserved for future use)
//!
//! Flutter projects must use buildFlutterApplication instead, so we detect
//! and skip them with a clear warning message.

use log::{debug, warn};
use std::path::Path;

const LOG_TARGET: &str = "nix-template::dart_deps";

/// Check if a Dart project is a Flutter project (which should be excluded).
///
/// Flutter projects have:
/// - dependencies.flutter with sdk: flutter
/// - dev_dependencies.flutter_test
///
/// Returns true if Flutter is detected (project should be skipped).
pub fn is_flutter_project(pubspec_yaml_path: &Path) -> bool {
    let contents = match std::fs::read_to_string(pubspec_yaml_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read pubspec.yaml: {}", e
            );
            return false;
        }
    };

    // Parse YAML
    let yaml: serde_yaml::Value = match serde_yaml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to parse pubspec.yaml: {}", e
            );
            return false;
        }
    };

    // Check dependencies.flutter.sdk == "flutter"
    if let Some(deps) = yaml.get("dependencies") {
        if let Some(flutter) = deps.get("flutter") {
            if let Some(sdk) = flutter.get("sdk") {
                if sdk.as_str() == Some("flutter") {
                    debug!(target: LOG_TARGET, "detected Flutter dependency (sdk: flutter)");
                    return true;
                }
            }
        }
    }

    // Check dev_dependencies.flutter_test
    if let Some(dev_deps) = yaml.get("dev_dependencies") {
        if dev_deps.get("flutter_test").is_some() {
            debug!(target: LOG_TARGET, "detected flutter_test in dev_dependencies");
            return true;
        }
    }

    false
}

/// Parse executable names from pubspec.yaml.
///
/// The executables section maps executable names to their entry points:
/// ```yaml
/// executables:
///   myapp: main
///   tool: tool
/// ```
///
/// Returns a list of executable names (e.g., ["myapp", "tool"]).
pub fn parse_dart_executables(pubspec_yaml_path: &Path) -> Vec<String> {
    let contents = match std::fs::read_to_string(pubspec_yaml_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read pubspec.yaml: {}", e
            );
            return Vec::new();
        }
    };

    // Parse YAML
    let yaml: serde_yaml::Value = match serde_yaml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to parse pubspec.yaml: {}", e
            );
            return Vec::new();
        }
    };

    // Extract executables section
    let executables_map = match yaml.get("executables") {
        Some(serde_yaml::Value::Mapping(map)) => map,
        Some(_) => {
            warn!(
                target: LOG_TARGET,
                "executables section exists but is not a mapping"
            );
            return Vec::new();
        }
        None => {
            debug!(target: LOG_TARGET, "no executables section found");
            return Vec::new();
        }
    };

    // Extract keys (executable names)
    let mut executables = Vec::new();
    for (key, _value) in executables_map {
        if let Some(name) = key.as_str() {
            executables.push(name.to_string());
            debug!(target: LOG_TARGET, "found executable: {}", name);
        }
    }

    executables
}

/// Extract Dart SDK version constraint from pubspec.yaml.
///
/// Returns the SDK version string (e.g., ">=3.0.0 <4.0.0") or None.
/// This is reserved for future version pinning support.
#[allow(dead_code)]
pub fn extract_dart_version(pubspec_yaml_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(pubspec_yaml_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let yaml: serde_yaml::Value = match serde_yaml::from_str(&contents) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Extract environment.sdk
    yaml.get("environment")?
        .get("sdk")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flutter_detection_dependencies() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_flutter_app
description: A Flutter app

dependencies:
  flutter:
    sdk: flutter
  cupertino_icons: ^1.0.2
"#,
        )
        .unwrap();

        assert!(is_flutter_project(&pubspec));
    }

    #[test]
    fn test_flutter_detection_dev_dependencies() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_flutter_app
description: A Flutter app

dev_dependencies:
  flutter_test:
    sdk: flutter
"#,
        )
        .unwrap();

        assert!(is_flutter_project(&pubspec));
    }

    #[test]
    fn test_pure_dart_not_flutter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_dart_cli
description: A pure Dart CLI tool

dependencies:
  args: ^2.0.0
  http: ^0.13.0
"#,
        )
        .unwrap();

        assert!(!is_flutter_project(&pubspec));
    }

    #[test]
    fn test_parse_executables() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_dart_app
description: A Dart application

executables:
  myapp: main
  tool: tool
  helper:
"#,
        )
        .unwrap();

        let executables = parse_dart_executables(&pubspec);
        assert_eq!(executables.len(), 3);
        assert!(executables.contains(&"myapp".to_string()));
        assert!(executables.contains(&"tool".to_string()));
        assert!(executables.contains(&"helper".to_string()));
    }

    #[test]
    fn test_parse_executables_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_dart_lib
description: A Dart library

dependencies:
  http: ^0.13.0
"#,
        )
        .unwrap();

        let executables = parse_dart_executables(&pubspec);
        assert!(executables.is_empty());
    }

    #[test]
    fn test_extract_dart_version() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pubspec = temp_dir.path().join("pubspec.yaml");
        std::fs::write(
            &pubspec,
            r#"
name: my_dart_app
description: A Dart application

environment:
  sdk: '>=3.0.0 <4.0.0'
"#,
        )
        .unwrap();

        let version = extract_dart_version(&pubspec);
        assert_eq!(version, Some(">=3.0.0 <4.0.0".to_string()));
    }
}

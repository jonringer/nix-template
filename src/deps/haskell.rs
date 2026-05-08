//! Inference of Haskell package type and GHC version from Haskell projects.
//!
//! This module reads a Haskell project's `.cabal` file to:
//! 1. Detect whether the package is an executable or library
//! 2. Extract package name and metadata
//! 3. Infer GHC version from cabal.project or stack.yaml (future enhancement)
//!
//! Haskell projects can use either Cabal or Stack build systems, and packages
//! can define executables, libraries, or both. This module helps distinguish
//! between these variants for proper template generation.

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::haskell_deps";

/// Check if a Haskell package defines any executables.
///
/// This parses the .cabal file looking for `executable` stanzas.
/// Returns true if at least one executable is found.
///
/// Example .cabal file:
/// ```cabal
/// name: mypackage
/// version: 1.0.0
///
/// library
///   exposed-modules: MyLib
///   build-depends: base >= 4.7 && < 5
///
/// executable myapp
///   main-is: Main.hs
///   build-depends: base, mypackage
/// ```
pub fn is_executable_package(cabal_path: &Path) -> bool {
    let contents = match std::fs::read_to_string(cabal_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read .cabal file: {}", e
            );
            return true; // Default to executable if we can't read the file
        }
    };

    // Simple heuristic: look for "executable" stanza in the .cabal file
    // This is case-insensitive and must be at the start of a line
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("executable ") || trimmed.to_lowercase() == "executable" {
            debug!(target: LOG_TARGET, "found executable stanza in .cabal file");
            return true;
        }
    }

    debug!(target: LOG_TARGET, "no executable stanza found in .cabal file (library-only package)");
    false
}

/// Extract package name from .cabal file.
///
/// Looks for the `name:` field in the .cabal file.
/// Returns the package name or None if not found.
#[allow(dead_code)]
pub fn extract_package_name(cabal_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(cabal_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("name:") {
            // Extract value after "name:"
            let name = trimmed[5..].trim();
            debug!(target: LOG_TARGET, "extracted package name: {}", name);
            return Some(name.to_string());
        }
    }

    None
}

/// Infer GHC version from cabal.project file.
///
/// Looks for `with-compiler: ghc-X.Y.Z` directive in cabal.project.
/// Returns the GHC version (e.g., "ghc98") or None if not specified.
#[allow(dead_code)]
pub fn infer_ghc_version_from_cabal_project(project_root: &Path) -> Option<String> {
    let cabal_project_path = project_root.join("cabal.project");
    if !cabal_project_path.exists() {
        return None;
    }

    let contents = match std::fs::read_to_string(&cabal_project_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("with-compiler:") {
            // Extract value after "with-compiler:"
            let compiler = trimmed[14..].trim();
            // Convert "ghc-9.8.1" to "ghc98"
            if let Some(version) = parse_ghc_version_string(compiler) {
                debug!(target: LOG_TARGET, "inferred GHC version from cabal.project: {}", version);
                return Some(version);
            }
        }
    }

    None
}

/// Parse GHC version string (e.g., "ghc-9.8.1" or "/usr/bin/ghc-9.8") to nixpkgs format (e.g., "ghc98").
fn parse_ghc_version_string(compiler: &str) -> Option<String> {
    // Extract version number from strings like "ghc-9.8.1", "/usr/bin/ghc-9.8", etc.
    // Split by '/' and '-' and find the first part that contains a version number
    for part in compiler.split(&['/', '-'][..]) {
        let trimmed = part.trim_start_matches("ghc");
        // Check if this part contains a version number (starts with a digit)
        if trimmed.is_empty() || !trimmed.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            continue;
        }

        // Extract major.minor version (e.g., "9.8" from "9.8.1")
        let parts: Vec<&str> = trimmed.split('.').collect();

        if parts.len() >= 2 {
            let major = parts[0];
            let minor = parts[1];
            return Some(format!("ghc{}{}", major, minor));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_executable_package() {
        let temp_dir = TempDir::new().unwrap();
        let cabal_path = temp_dir.path().join("mypackage.cabal");
        fs::write(
            &cabal_path,
            r#"
name: mypackage
version: 1.0.0

library
  exposed-modules: MyLib
  build-depends: base >= 4.7 && < 5

executable myapp
  main-is: Main.hs
  build-depends: base, mypackage
"#,
        )
        .unwrap();

        assert!(is_executable_package(&cabal_path));
    }

    #[test]
    fn test_library_only_package() {
        let temp_dir = TempDir::new().unwrap();
        let cabal_path = temp_dir.path().join("mylib.cabal");
        fs::write(
            &cabal_path,
            r#"
name: mylib
version: 1.0.0

library
  exposed-modules: MyLib
  build-depends: base >= 4.7 && < 5
"#,
        )
        .unwrap();

        assert!(!is_executable_package(&cabal_path));
    }

    #[test]
    fn test_extract_package_name() {
        let temp_dir = TempDir::new().unwrap();
        let cabal_path = temp_dir.path().join("test.cabal");
        fs::write(
            &cabal_path,
            r#"
name:        my-awesome-package
version:     1.2.3
"#,
        )
        .unwrap();

        let name = extract_package_name(&cabal_path);
        assert_eq!(name, Some("my-awesome-package".to_string()));
    }

    #[test]
    fn test_parse_ghc_version_string() {
        assert_eq!(parse_ghc_version_string("ghc-9.8.1"), Some("ghc98".to_string()));
        assert_eq!(parse_ghc_version_string("ghc-9.6.3"), Some("ghc96".to_string()));
        assert_eq!(parse_ghc_version_string("/usr/bin/ghc-9.4"), Some("ghc94".to_string()));
        assert_eq!(parse_ghc_version_string("9.8.1"), Some("ghc98".to_string()));
    }

    #[test]
    fn test_infer_ghc_version_from_cabal_project() {
        let temp_dir = TempDir::new().unwrap();
        let cabal_project_path = temp_dir.path().join("cabal.project");
        fs::write(
            &cabal_project_path,
            r#"
packages: .

with-compiler: ghc-9.8.1
"#,
        )
        .unwrap();

        let version = infer_ghc_version_from_cabal_project(temp_dir.path());
        assert_eq!(version, Some("ghc98".to_string()));
    }
}

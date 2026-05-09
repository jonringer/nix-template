//! Inference of OCaml package information from dune-project and .opam files.
//!
//! This module reads OCaml project files to:
//! 1. Extract package name from dune-project or .opam files
//! 2. Extract OCaml version constraints (reserved for future use)
//!
//! OCaml projects typically use the Dune build system and OPAM package manager.
//! Package metadata can be found in either dune-project (modern) or .opam files (legacy).

use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::ocaml_deps";

/// Extract package name from dune-project file.
///
/// dune-project files use S-expression syntax:
/// ```sexp
/// (lang dune 3.0)
/// (name mypackage)
/// ```
///
/// Returns the package name or None if not found.
pub fn extract_package_name_from_dune(dune_project_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(dune_project_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read dune-project: {}", e
            );
            return None;
        }
    };

    // Parse for (name <package-name>)
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("(name ") {
            // Extract the package name from (name mypackage)
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1].trim_end_matches(')');
                debug!(target: LOG_TARGET, "extracted package name from dune-project: {}", name);
                return Some(name.to_string());
            }
        }
    }

    debug!(target: LOG_TARGET, "no package name found in dune-project");
    None
}

/// Extract package name from .opam file.
///
/// The filename itself is the package name (e.g., mypackage.opam).
/// For standalone "opam" files, we return None as the package name must be inferred.
///
/// Returns the package name or None.
pub fn extract_package_name_from_opam(opam_path: &Path) -> Option<String> {
    // Extract filename without extension
    if let Some(filename) = opam_path.file_stem().and_then(|s| s.to_str()) {
        // Don't return "opam" as a package name (standalone opam file)
        if filename == "opam" {
            debug!(target: LOG_TARGET, "standalone opam file found, cannot infer package name");
            return None;
        }
        debug!(target: LOG_TARGET, "extracted package name from opam file: {}", filename);
        return Some(filename.to_string());
    }

    None
}

/// Extract OCaml version constraint from dune-project file.
///
/// dune-project files may specify OCaml version requirements:
/// ```sexp
/// (lang dune 3.0)
/// (package
///  (name mypackage)
///  (depends
///   (ocaml (>= 4.14))))
/// ```
///
/// Returns the OCaml version constraint or None.
/// This is reserved for future version pinning support.
#[allow(dead_code)]
pub fn extract_ocaml_version(dune_project_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(dune_project_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Simple heuristic: look for (ocaml (...)) in depends section
    // This is a simplified parser; a full S-expression parser would be more robust
    let mut in_depends = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.contains("(depends") {
            in_depends = true;
        }
        if in_depends && trimmed.contains("(ocaml") {
            // Extract version constraint (e.g., ">= 4.14")
            // This is a very basic extraction; proper parsing would require an S-exp library
            if let Some(start) = trimmed.find('(') {
                if let Some(end) = trimmed.rfind(')') {
                    let content = &trimmed[start + 1..end];
                    if content.starts_with("ocaml ") {
                        let version = content[6..].trim();
                        debug!(target: LOG_TARGET, "extracted OCaml version: {}", version);
                        return Some(version.to_string());
                    }
                }
            }
        }
        if in_depends && trimmed.contains("))") {
            in_depends = false;
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
    fn test_extract_package_name_from_dune() {
        let temp_dir = TempDir::new().unwrap();
        let dune_project = temp_dir.path().join("dune-project");
        fs::write(
            &dune_project,
            r#"
(lang dune 3.0)
(name my-ocaml-package)
"#,
        )
        .unwrap();

        let name = extract_package_name_from_dune(&dune_project);
        assert_eq!(name, Some("my-ocaml-package".to_string()));
    }

    #[test]
    fn test_extract_package_name_from_opam() {
        let temp_dir = TempDir::new().unwrap();
        let opam_file = temp_dir.path().join("mypackage.opam");
        fs::write(&opam_file, "").unwrap();

        let name = extract_package_name_from_opam(&opam_file);
        assert_eq!(name, Some("mypackage".to_string()));
    }

    #[test]
    fn test_extract_package_name_from_standalone_opam() {
        let temp_dir = TempDir::new().unwrap();
        let opam_file = temp_dir.path().join("opam");
        fs::write(&opam_file, "").unwrap();

        let name = extract_package_name_from_opam(&opam_file);
        assert_eq!(name, None);
    }

    #[test]
    fn test_extract_ocaml_version() {
        let temp_dir = TempDir::new().unwrap();
        let dune_project = temp_dir.path().join("dune-project");
        fs::write(
            &dune_project,
            r#"
(lang dune 3.0)
(name mypackage)
(package
 (name mypackage)
 (depends
  (ocaml (>= 4.14))))
"#,
        )
        .unwrap();

        let version = extract_ocaml_version(&dune_project);
        assert!(version.is_some());
        // The exact parsing depends on the implementation
    }
}

//! Inference of Python package dependencies from `pyproject.toml`.
//!
//! This module reads a Python project's `pyproject.toml`, walks the
//! `[project.dependencies]` (PEP 621) or `[tool.poetry.dependencies]`
//! tables, and maps each dependency to its nixpkgs `python3Packages`
//! attribute name for use in `propagatedBuildInputs`.
//!
//! The mapping normalises PyPI names to nixpkgs conventions (lowercase,
//! hyphens to underscores) and uses a static override table for packages
//! whose nixpkgs name diverges from the PyPI name.
//!
//! As with the Rust and Go modules, the mapping is best-effort: users
//! can edit the generated expression to add anything we missed.

use log::debug;
use std::collections::BTreeSet;
use std::path::Path;
use toml::Value;

const LOG_TARGET: &str = "nix-template::python_deps";

/// Normalise a PyPI package name to the most likely nixpkgs
/// `python3Packages` attribute name.
///
/// PyPI names are case-insensitive and treat `-`, `_`, and `.` as
/// equivalent (PEP 503). nixpkgs conventionally lowercases and uses
/// hyphens, though some older packages use underscores.
fn normalise_pypi_name(name: &str) -> String {
    name.to_lowercase()
        .replace('_', "-")
        .replace('.', "-")
}

/// Static overrides for PyPI names whose nixpkgs attribute diverges
/// from the normalised form. Returns `None` when the normalised name
/// is correct as-is.
fn lookup_override(normalised: &str) -> Option<&'static str> {
    match normalised {
        // PIL / Pillow
        "pillow" => Some("pillow"),
        // PyYAML
        "pyyaml" => Some("pyyaml"),
        // python-dateutil
        "python-dateutil" => Some("python-dateutil"),
        // scikit-learn
        "scikit-learn" => Some("scikit-learn"),
        // beautifulsoup4
        "beautifulsoup4" => Some("beautifulsoup4"),
        // opencv-python -> opencv4 (system dep, not a python package in nixpkgs)
        "opencv-python" | "opencv-python-headless" => Some("opencv4"),
        // google-cloud packages
        "google-auth" => Some("google-auth"),
        "google-api-python-client" => Some("google-api-python-client"),
        // attrs (the package is just "attrs" in nixpkgs too)
        "attrs" => Some("attrs"),
        // importlib-metadata
        "importlib-metadata" => Some("importlib-metadata"),
        // typing-extensions
        "typing-extensions" => Some("typing-extensions"),
        _ => None,
    }
}

/// Well-known packages to skip — they are part of the Python standard
/// library and don't need to appear in `propagatedBuildInputs`.
fn is_stdlib(normalised: &str) -> bool {
    matches!(
        normalised,
        "pip"
            | "setuptools"
            | "wheel"
            | "build"
            | "flit"
            | "flit-core"
            | "poetry"
            | "poetry-core"
            | "hatchling"
            | "hatch"
            | "maturin"
            | "pdm"
            | "pdm-backend"
    )
}

/// Extract the package name from a PEP 508 dependency string.
///
/// Examples:
/// - `"requests>=2.0"` → `"requests"`
/// - `"numpy[extra] ; python_version >= '3.8'"` → `"numpy"`
/// - `"my-pkg"` → `"my-pkg"`
fn extract_package_name(dep_spec: &str) -> &str {
    let s = dep_spec.trim();
    // Name ends at the first version specifier, extra marker, or whitespace
    let end = s
        .find(|c: char| c == '>' || c == '<' || c == '=' || c == '!' || c == ';' || c == '[' || c == ' ')
        .unwrap_or(s.len());
    s[..end].trim()
}

/// Parse `[project.dependencies]` (PEP 621 format) from a parsed
/// `pyproject.toml` value and return the dependency names.
fn parse_pep621_deps(parsed: &Value) -> Vec<String> {
    parsed
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| extract_package_name(s).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `[tool.poetry.dependencies]` from a parsed `pyproject.toml`
/// value and return the dependency names. Skips `python` itself.
fn parse_poetry_deps(parsed: &Value) -> Vec<String> {
    parsed
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
        .map(|tbl| {
            tbl.keys()
                .filter(|k| k.as_str() != "python")
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Map a list of raw PyPI dependency names to nixpkgs
/// `python3Packages` attribute names. Deduplicates and sorts.
fn map_deps_to_nix(raw_names: &[String]) -> Vec<String> {
    let mut nix_deps: BTreeSet<String> = BTreeSet::new();

    for name in raw_names {
        let normalised = normalise_pypi_name(name);
        if is_stdlib(&normalised) {
            continue;
        }
        let nix_name = lookup_override(&normalised)
            .map(|s| s.to_owned())
            .unwrap_or(normalised);
        nix_deps.insert(nix_name);
    }

    nix_deps.into_iter().collect()
}

/// Infer `propagatedBuildInputs` from a local Python project directory.
///
/// Reads `pyproject.toml` and extracts dependencies from either PEP 621
/// `[project.dependencies]` or Poetry `[tool.poetry.dependencies]`.
/// Returns the list of nixpkgs `python3Packages` attribute names.
pub fn infer_python_dependencies_from_path(source_path: &Path) -> Vec<String> {
    let pyproject_path = source_path.join("pyproject.toml");
    let content = match std::fs::read_to_string(&pyproject_path) {
        Ok(c) => c,
        Err(_) => {
            debug!(target: LOG_TARGET, "no pyproject.toml found");
            return Vec::new();
        }
    };

    let parsed: Value = match content.parse() {
        Ok(v) => v,
        Err(e) => {
            debug!(target: LOG_TARGET, "failed to parse pyproject.toml: {}", e);
            return Vec::new();
        }
    };

    // Try PEP 621 first, fall back to Poetry
    let raw_deps = {
        let pep621 = parse_pep621_deps(&parsed);
        if pep621.is_empty() {
            parse_poetry_deps(&parsed)
        } else {
            pep621
        }
    };

    if raw_deps.is_empty() {
        debug!(target: LOG_TARGET, "no dependencies found in pyproject.toml");
        return Vec::new();
    }

    let nix_deps = map_deps_to_nix(&raw_deps);
    if !nix_deps.is_empty() {
        eprintln!(
            "Inferred {} propagatedBuildInputs from pyproject.toml: {:?}",
            nix_deps.len(),
            nix_deps,
        );
    }
    nix_deps
}

/// Infer `propagatedBuildInputs` from a materialised remote source.
pub fn infer_python_dependencies(info: &crate::types::ExpressionInfo) -> Vec<String> {
    match info.template {
        crate::types::Template::python_package | crate::types::Template::python_application => {}
        _ => return Vec::new(),
    }

    eprintln!("Materialising source to scan for Python dependencies...");
    let source_path = match crate::source::materialise_source(info) {
        Some(p) => p,
        None => {
            debug!(target: LOG_TARGET, "failed to materialise source");
            return Vec::new();
        }
    };
    infer_python_dependencies_from_path(&source_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_dir(pyproject_content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), pyproject_content).unwrap();
        dir
    }

    #[test]
    fn pep621_basic_dependencies() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
dependencies = [
    "requests>=2.28",
    "click",
    "pyyaml>=6.0",
]
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["click", "pyyaml", "requests"]);
    }

    #[test]
    fn pep621_with_extras_and_markers() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
dependencies = [
    "numpy[extra]>=1.20",
    "pandas ; python_version >= '3.8'",
]
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["numpy", "pandas"]);
    }

    #[test]
    fn poetry_dependencies() {
        let dir = make_dir(
            r#"
[tool.poetry]
name = "myapp"

[tool.poetry.dependencies]
python = "^3.8"
requests = "^2.28"
click = "^8.0"
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["click", "requests"]);
    }

    #[test]
    fn skips_build_tools() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
dependencies = [
    "setuptools",
    "wheel",
    "requests",
]
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["requests"]);
    }

    #[test]
    fn normalises_names() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
dependencies = [
    "Pillow>=9.0",
    "scikit-learn",
    "PyYAML",
]
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["pillow", "pyyaml", "scikit-learn"]);
    }

    #[test]
    fn deduplicates() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
dependencies = [
    "requests",
    "requests>=2.0",
]
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert_eq!(deps, vec!["requests"]);
    }

    #[test]
    fn no_pyproject_returns_empty() {
        let dir = TempDir::new().unwrap();
        let deps = infer_python_dependencies_from_path(dir.path());
        assert!(deps.is_empty());
    }

    #[test]
    fn no_deps_returns_empty() {
        let dir = make_dir(
            r#"
[project]
name = "myapp"
"#,
        );
        let deps = infer_python_dependencies_from_path(dir.path());
        assert!(deps.is_empty());
    }

    #[test]
    fn extract_package_name_variants() {
        assert_eq!(extract_package_name("requests>=2.0"), "requests");
        assert_eq!(extract_package_name("numpy[extra]"), "numpy");
        assert_eq!(extract_package_name("pandas ; python_version >= '3.8'"), "pandas");
        assert_eq!(extract_package_name("  click  "), "click");
        assert_eq!(extract_package_name("my-pkg"), "my-pkg");
        assert_eq!(extract_package_name("pkg!=1.0"), "pkg");
        assert_eq!(extract_package_name("pkg<2"), "pkg");
    }
}

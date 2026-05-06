//! Automatic template detection from source tree contents.
//!
//! Given a populated `ExpressionInfo` (with a known source hash), materialise
//! the source and scan for indicator files that reveal the project's build
//! system. Returns a list of candidate templates ordered by priority.

use crate::source;
use crate::types::{ExpressionInfo, Fetcher, Template};
use log::debug;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::detect";

/// A detected template candidate with the file that triggered it.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub template: Template,
    pub reason: &'static str,
}

/// Indicator files and their associated templates, in priority order.
/// When multiple indicators are present, the priority determines the
/// non-interactive fallback (first match wins).
const INDICATORS: &[(&str, Template, &str)] = &[
    ("Cargo.toml", Template::rust, "Cargo.toml"),
    ("go.mod", Template::go, "go.mod"),
    ("pyproject.toml", Template::python_package, "pyproject.toml"),
    ("setup.py", Template::python_package, "setup.py"),
    ("setup.cfg", Template::python_package, "setup.cfg"),
    ("meson.build", Template::stdenv, "meson.build"),
    ("CMakeLists.txt", Template::stdenv, "CMakeLists.txt"),
    ("configure", Template::stdenv, "configure"),
    ("configure.ac", Template::stdenv, "configure.ac"),
    ("Makefile", Template::stdenv, "Makefile"),
];

/// Scan a directory for build-system indicator files and return template candidates.
///
/// This is the core detection logic, usable with both local paths (e.g., the
/// current working directory for `--init-*` flags) and materialised remote sources.
///
/// Multiple candidates are returned when several indicator files are found
/// from *different* template categories (e.g., `Cargo.toml` + `pyproject.toml`).
/// Duplicate template entries (e.g., both `setup.py` and `pyproject.toml` for
/// python) are deduplicated, keeping the first-seen reason.
pub fn detect_template_candidates_from_path(source_path: &Path) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen_templates: Vec<Template> = Vec::new();

    for &(filename, ref template, reason) in INDICATORS {
        if source_path.join(filename).exists() {
            // Deduplicate by template type (e.g., setup.py and pyproject.toml
            // both map to python_package — only keep the first).
            if seen_templates.contains(template) {
                continue;
            }
            seen_templates.push(template.clone());
            candidates.push(Candidate {
                template: template.clone(),
                reason,
            });
        }
    }

    // Python sub-classification: if python was detected, check if it's an application.
    for candidate in candidates.iter_mut() {
        if candidate.template == Template::python_package {
            if is_python_application(source_path) {
                candidate.template = Template::python_application;
            }
            break;
        }
    }

    candidates
}

/// Detect template candidates by materialising a remote source tree.
///
/// If the fetcher is PyPI, short-circuits without materialising (we already
/// know it's Python). Otherwise fetches the source into the Nix store and
/// delegates to `detect_template_candidates_from_path`.
pub fn detect_template_candidates(info: &ExpressionInfo) -> Vec<Candidate> {
    // PyPI short-circuit: we know it's Python, just classify package vs application.
    if info.fetcher == Fetcher::pypi {
        return vec![Candidate {
            template: Template::python_package,
            reason: "PyPI source",
        }];
    }

    eprintln!("Materialising source to detect project type...");
    let source_path = match source::materialise_source(info) {
        Some(p) => p,
        None => {
            debug!(target: LOG_TARGET, "failed to materialise source; cannot detect template");
            return Vec::new();
        }
    };

    detect_template_candidates_from_path(&source_path)
}

/// Determine whether a Python project is an application (has entry points /
/// scripts) or a library (no scripts).
fn is_python_application(source: &Path) -> bool {
    // Check pyproject.toml for [project.scripts] or [project.gui-scripts]
    let pyproject_path = source.join("pyproject.toml");
    if pyproject_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
            if let Ok(parsed) = content.parse::<toml::Value>() {
                // [project.scripts] or [project.gui-scripts]
                if let Some(project) = parsed.get("project") {
                    if has_non_empty_table(project, "scripts")
                        || has_non_empty_table(project, "gui-scripts")
                    {
                        return true;
                    }
                }
                // [tool.poetry.scripts]
                if let Some(tool) = parsed.get("tool") {
                    if let Some(poetry) = tool.get("poetry") {
                        if has_non_empty_table(poetry, "scripts") {
                            return true;
                        }
                    }
                }
            }
        }
    }

    // Check setup.cfg for [options.entry_points] console_scripts
    let setup_cfg_path = source.join("setup.cfg");
    if setup_cfg_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&setup_cfg_path) {
            // Simple heuristic: look for console_scripts in the file
            if content.contains("console_scripts") {
                return true;
            }
        }
    }

    false
}

/// Check if a TOML value has a non-empty table or array at the given key.
fn has_non_empty_table(value: &toml::Value, key: &str) -> bool {
    match value.get(key) {
        Some(toml::Value::Table(t)) => !t.is_empty(),
        Some(toml::Value::Array(a)) => !a.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_source_dir(files: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for f in files {
            let path = dir.path().join(f);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        dir
    }

    #[test]
    fn detect_rust_from_cargo_toml() {
        let dir = make_source_dir(&["Cargo.toml", "src/main.rs"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::rust);
    }

    #[test]
    fn detect_go_from_go_mod() {
        let dir = make_source_dir(&["go.mod", "main.go"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::go);
    }

    #[test]
    fn detect_python_from_pyproject() {
        let dir = make_source_dir(&["pyproject.toml", "src/mypackage/__init__.py"]);
        // Write a minimal pyproject without scripts
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"mypackage\"\n",
        )
        .unwrap();
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::python_package);
    }

    #[test]
    fn detect_python_application_from_scripts() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"myapp\"\n\n[project.scripts]\nmyapp = \"myapp:main\"\n",
        )
        .unwrap();
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::python_application);
    }

    #[test]
    fn detect_multiple_candidates() {
        let dir = make_source_dir(&["Cargo.toml", "pyproject.toml"]);
        fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"x\"\n").unwrap();
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].template, Template::rust);
        assert_eq!(candidates[1].template, Template::python_package);
    }

    #[test]
    fn deduplicate_python_indicators() {
        let dir = make_source_dir(&["pyproject.toml", "setup.py", "setup.cfg"]);
        fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"x\"\n").unwrap();
        let candidates = detect_template_candidates_from_path(dir.path());
        // Only one python entry despite three indicator files
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::python_package);
    }

    #[test]
    fn detect_stdenv_from_cmake() {
        let dir = make_source_dir(&["CMakeLists.txt", "src/main.c"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::stdenv);
    }

    #[test]
    fn no_indicators_returns_empty() {
        let dir = make_source_dir(&["README.md", "data/font.ttf"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert!(candidates.is_empty());
    }
}

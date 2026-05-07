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
    ("pnpm-lock.yaml", Template::pnpm, "pnpm-lock.yaml"),
    ("package-lock.json", Template::npm, "package-lock.json"),
    // Note: package.json is handled separately as a fallback (see below)
    ("Gemfile.lock", Template::ruby, "Gemfile.lock"),
    ("Gemfile", Template::ruby, "Gemfile"),
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

    // npm/pnpm fallback: if no lockfile was found but package.json exists, use npm.
    // This only runs if neither pnpm-lock.yaml nor package-lock.json were detected.
    let has_npm_or_pnpm = candidates
        .iter()
        .any(|c| c.template == Template::npm || c.template == Template::pnpm);

    if !has_npm_or_pnpm && source_path.join("package.json").exists() {
        candidates.push(Candidate {
            template: Template::npm,
            reason: "package.json",
        });
    }

    // .NET detection: scan for .csproj, .fsproj, or .sln files
    // These files have dynamic names (e.g., MyProject.csproj), so we need to scan the directory.
    let has_dotnet = candidates
        .iter()
        .any(|c| c.template == Template::dotnet);

    if !has_dotnet {
        if let Some(reason) = find_dotnet_project_file(source_path) {
            candidates.push(Candidate {
                template: Template::dotnet,
                reason,
            });
        }
    }

    candidates
}

/// Scan a directory for .NET project files (.csproj, .fsproj, .sln).
/// Returns the reason string (file type found) or None if no project files exist.
fn find_dotnet_project_file(source_path: &Path) -> Option<&'static str> {
    use std::fs;

    if let Ok(entries) = fs::read_dir(source_path) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                match ext {
                    "csproj" => return Some("*.csproj"),
                    "fsproj" => return Some("*.fsproj"),
                    "sln" => return Some("*.sln"),
                    _ => {}
                }
            }
        }
    }
    None
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

/// Detect the Python build system format from `pyproject.toml`.
///
/// Parses `[build-system].requires` to identify the build backend and returns
/// the corresponding nixpkgs `format` value. Falls back to `"setuptools"` when
/// detection fails or no pyproject.toml is present.
pub fn detect_python_format(source_path: &Path) -> String {
    let pyproject_path = source_path.join("pyproject.toml");
    if !pyproject_path.is_file() {
        return "setuptools".to_owned();
    }

    let content = match std::fs::read_to_string(&pyproject_path) {
        Ok(c) => c,
        Err(_) => return "setuptools".to_owned(),
    };

    let parsed = match content.parse::<toml::Value>() {
        Ok(v) => v,
        Err(_) => return "setuptools".to_owned(),
    };

    // Look at [build-system].requires
    let requires = parsed
        .get("build-system")
        .and_then(|bs| bs.get("requires"))
        .and_then(|r| r.as_array());

    let requires = match requires {
        Some(r) => r,
        None => {
            // pyproject.toml exists but no [build-system] section
            return "pyproject".to_owned();
        }
    };

    // Check each requirement against known backends
    for req in requires {
        if let Some(s) = req.as_str() {
            let lower = s.to_lowercase();
            // Extract package name (before any version specifier)
            let pkg = lower.split(&['>', '<', '=', '!', ';', '['][..]).next().unwrap_or("");
            let pkg = pkg.trim();
            match pkg {
                "flit_core" | "flit" => return "flit".to_owned(),
                "poetry-core" | "poetry" => return "poetry".to_owned(),
                "hatchling" | "hatch" => return "hatchling".to_owned(),
                "maturin" => return "setuptools".to_owned(),
                "setuptools" => return "setuptools".to_owned(),
                _ => {}
            }
        }
    }

    // [build-system].requires exists but no known backend matched
    "pyproject".to_owned()
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

    #[test]
    fn detect_npm_from_package_lock() {
        let dir = make_source_dir(&["package.json", "package-lock.json"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::npm);
        assert_eq!(candidates[0].reason, "package-lock.json");
    }

    #[test]
    fn detect_pnpm_from_pnpm_lock() {
        let dir = make_source_dir(&["package.json", "pnpm-lock.yaml"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::pnpm);
        assert_eq!(candidates[0].reason, "pnpm-lock.yaml");
    }

    #[test]
    fn detect_npm_from_package_json_fallback() {
        let dir = make_source_dir(&["package.json"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::npm);
        assert_eq!(candidates[0].reason, "package.json");
    }

    #[test]
    fn prefer_pnpm_when_both_package_json_and_pnpm_lock() {
        // This tests the sub-classification logic: if package.json would trigger npm
        // but pnpm-lock.yaml also exists, we prefer pnpm.
        let dir = make_source_dir(&["package.json", "pnpm-lock.yaml"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        // pnpm-lock.yaml has higher priority in INDICATORS, so it should win
        assert_eq!(candidates[0].template, Template::pnpm);
    }

    #[test]
    fn deduplicate_npm_indicators() {
        // When both package-lock.json and package.json exist, only the first should be kept
        let dir = make_source_dir(&["package.json", "package-lock.json"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::npm);
        // package-lock.json has higher priority than package.json
        assert_eq!(candidates[0].reason, "package-lock.json");
    }

    #[test]
    fn detect_multiple_with_npm() {
        let dir = make_source_dir(&["Cargo.toml", "package.json"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].template, Template::rust);
        assert_eq!(candidates[1].template, Template::npm);
    }

    #[test]
    fn detect_dotnet_from_csproj() {
        let dir = make_source_dir(&["MyProject.csproj", "Program.cs"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::dotnet);
        assert_eq!(candidates[0].reason, "*.csproj");
    }

    #[test]
    fn detect_dotnet_from_fsproj() {
        let dir = make_source_dir(&["MyProject.fsproj", "Program.fs"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::dotnet);
        assert_eq!(candidates[0].reason, "*.fsproj");
    }

    #[test]
    fn detect_dotnet_from_sln() {
        let dir = make_source_dir(&["MySolution.sln", "README.md"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::dotnet);
        assert_eq!(candidates[0].reason, "*.sln");
    }

    #[test]
    fn detect_multiple_with_dotnet() {
        let dir = make_source_dir(&["Cargo.toml", "MyProject.csproj"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].template, Template::rust);
        assert_eq!(candidates[1].template, Template::dotnet);
    }

    #[test]
    fn detect_ruby_from_gemfile_lock() {
        let dir = make_source_dir(&["Gemfile", "Gemfile.lock", "lib/app.rb"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::ruby);
        assert_eq!(candidates[0].reason, "Gemfile.lock");
    }

    #[test]
    fn detect_ruby_from_gemfile_fallback() {
        let dir = make_source_dir(&["Gemfile", "lib/app.rb"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::ruby);
        assert_eq!(candidates[0].reason, "Gemfile");
    }

    #[test]
    fn detect_multiple_with_ruby() {
        let dir = make_source_dir(&["Cargo.toml", "Gemfile"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].template, Template::rust);
        assert_eq!(candidates[1].template, Template::ruby);
    }

    #[test]
    fn deduplicate_ruby_indicators() {
        // When both Gemfile.lock and Gemfile exist, only Gemfile.lock should be kept
        let dir = make_source_dir(&["Gemfile", "Gemfile.lock"]);
        let candidates = detect_template_candidates_from_path(dir.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template, Template::ruby);
        // Gemfile.lock has higher priority than Gemfile
        assert_eq!(candidates[0].reason, "Gemfile.lock");
    }

    #[test]
    fn detect_format_setuptools() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"setuptools>=61.0\"]\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "setuptools");
    }

    #[test]
    fn detect_format_flit() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"flit_core>=3.2\"]\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "flit");
    }

    #[test]
    fn detect_format_poetry() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"poetry-core>=1.0.0\"]\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "poetry");
    }

    #[test]
    fn detect_format_hatchling() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"hatchling\"]\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "hatchling");
    }

    #[test]
    fn detect_format_maturin_maps_to_setuptools() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"maturin>=1.0\"]\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "setuptools");
    }

    #[test]
    fn detect_format_no_build_system_returns_pyproject() {
        let dir = make_source_dir(&["pyproject.toml"]);
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"mypackage\"\n",
        )
        .unwrap();
        assert_eq!(detect_python_format(dir.path()), "pyproject");
    }

    #[test]
    fn detect_format_no_pyproject_returns_setuptools() {
        let dir = make_source_dir(&["setup.py"]);
        assert_eq!(detect_python_format(dir.path()), "setuptools");
    }
}

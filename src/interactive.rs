use crate::types::{Fetcher, GithubRepo, PypiRepo, Repo, Template, UserConfig};
use crate::url::{fetch_github_release_info, fetch_github_repo_info, fetch_pypi_project_info};
use anyhow::{anyhow, Result};
use inquire::{Confirm, Select, Text};
use regex::Regex;
use std::collections::HashMap;
use version_compare::VersionCompare;

lazy_static! {
    static ref GITHUB_URL_REGEX: Regex = {
        Regex::new("github.com/([^/]*)/([^/]*)/?").unwrap()
    };
    static ref PYPI_URL_REGEX: Regex = {
        Regex::new("pypi.org/project/([^/]*)/?").unwrap()
    };
    static ref VERSION_REGEX: Regex = {
        Regex::new("^([^0-9]*)(.+)").unwrap()
    };
    static ref STABLE_RELEASE_REGEX: Regex = {
        Regex::new(r"^([0-9.]*)+$").unwrap()
    };
    static ref GITHUB_TO_NIXPKGS_LICENSE: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("agpl-3.0", "agpl3");
        m.insert("apache-2.0", "asl20");
        m.insert("bsd-2-clause", "bsd2");
        m.insert("bsd-3-clause", "bsd3");
        m.insert("bsl-1.0", "bsl11");
        m.insert("cc0-1.0", "cc0");
        m.insert("epl-2.0", "epl20");
        m.insert("gpl-2.0", "gpl2");
        m.insert("gpl-3.0", "gpl3");
        m.insert("lgpl-2.1", "lgpl21");
        m.insert("mit", "mit");
        m.insert("mpl-2.0", "mpl20");
        m.insert("unlicense", "unlicense");
        m
    };
    static ref PYPI_TO_NIXPKGS_LICENSE: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("Apache 2.0", "asl20");
        m.insert("Apache Software License", "asl20");
        m.insert("BSD-3-clause", "bsd3");
        m.insert("MIT License", "mit");
        m
    };
}

const COMMON_LICENSES: &[&str] = &[
    "mit",
    "asl20",
    "gpl3",
    "gpl2",
    "lgpl21",
    "bsd3",
    "bsd2",
    "agpl3",
    "mpl20",
    "cc0",
    "unlicense",
    "Custom (enter manually)",
];

/// Metadata extracted from a URL (GitHub or PyPI)
#[derive(Debug, Clone)]
pub struct UrlMetadata {
    pub pname: String,
    pub license: String,
    pub description: String,
    pub homepage: String,
    pub fetcher: Fetcher,
    pub owner: Option<String>,
}

impl Default for UrlMetadata {
    fn default() -> Self {
        Self {
            pname: "CHANGE".to_string(),
            license: "CHANGE".to_string(),
            description: "CHANGE".to_string(),
            homepage: "".to_string(),
            fetcher: Fetcher::github,
            owner: None,
        }
    }
}

/// Prompt for template type
pub fn prompt_template_type(default: Option<Template>) -> Result<Template> {
    let options = vec![
        ("stdenv", "Standard environment derivation"),
        ("python", "Python package with buildPythonPackage"),
        ("rust", "Rust package with rustPlatform.buildRustPackage"),
        ("go", "Go package with buildGoModule"),
        ("qt", "Qt application with mkDerivation"),
        ("mkshell", "Development shell.nix"),
        ("flake", "Nix flake template"),
        ("module", "NixOS module"),
        ("test", "NixOS test"),
    ];

    let display_options: Vec<String> = options
        .iter()
        .map(|(name, desc)| format!("{:<10} - {}", name, desc))
        .collect();

    let default_idx = if let Some(template) = default {
        options.iter().position(|(name, _)| {
            let template_str = format!("{:?}", template);
            *name == template_str.as_str()
        })
    } else {
        Some(0) // Default to stdenv
    };

    let selection = Select::new("Select template type:", display_options)
        .with_starting_cursor(default_idx.unwrap_or(0))
        .prompt()?;

    // Extract template name from the formatted string
    let template_name = selection.split_whitespace().next().unwrap();

    Ok(match template_name {
        "stdenv" => Template::stdenv,
        "python" => Template::python,
        "rust" => Template::rust,
        "go" => Template::go,
        "qt" => Template::qt,
        "mkshell" => Template::mkshell,
        "flake" => Template::flake,
        "module" => Template::module,
        "test" => Template::test,
        _ => Template::stdenv,
    })
}

/// Extract metadata from a URL
pub fn extract_metadata_from_url(url: &str) -> Result<UrlMetadata> {
    let repo = parse_url(url)?;

    match repo {
        Repo::Github(gh_repo) => {
            eprintln!("Fetching metadata from GitHub for {}/{}...", gh_repo.owner, gh_repo.repo);

            let repo_info = fetch_github_repo_info(&gh_repo);

            let license = if repo_info.license.key != "other" {
                GITHUB_TO_NIXPKGS_LICENSE
                    .get(&*repo_info.license.key)
                    .unwrap_or(&"CHANGE")
                    .to_string()
            } else {
                "CHANGE".to_string()
            };

            let homepage = format!("https://github.com/{}/{}", gh_repo.owner, gh_repo.repo);

            Ok(UrlMetadata {
                pname: gh_repo.repo.clone(),
                license,
                description: repo_info.description.unwrap_or("CHANGE".to_string()),
                homepage,
                fetcher: Fetcher::github,
                owner: Some(gh_repo.owner.clone()),
            })
        }
        Repo::Pypi(pypi_repo) => {
            eprintln!("Fetching metadata from PyPI for {}...", pypi_repo.project);

            let pypi_response = fetch_pypi_project_info(&pypi_repo);

            let license = PYPI_TO_NIXPKGS_LICENSE
                .get(&*pypi_response.info.license)
                .unwrap_or(&"CHANGE")
                .to_string();

            Ok(UrlMetadata {
                pname: pypi_repo.project.clone(),
                license,
                description: pypi_response.info.summary.trim_end_matches('.').to_string(),
                homepage: pypi_response.info.home_page.clone(),
                fetcher: Fetcher::pypi,
                owner: None,
            })
        }
    }
}

/// Prompt for URL (GitHub or PyPI) and return URL with extracted metadata
pub fn prompt_url() -> Result<Option<(String, UrlMetadata)>> {
    let should_provide = Confirm::new("Do you want to fetch metadata from a URL (GitHub/PyPI)?")
        .with_default(false)
        .prompt()?;

    if !should_provide {
        return Ok(None);
    }

    let url = Text::new("Enter GitHub or PyPI URL:")
        .with_help_message("Examples: github.com/owner/repo or pypi.org/project/package")
        .prompt()?;

    if url.is_empty() {
        return Ok(None);
    }

    // Immediately fetch metadata from the URL
    match extract_metadata_from_url(&url) {
        Ok(metadata) => {
            eprintln!("✓ Successfully fetched metadata!");
            Ok(Some((url, metadata)))
        }
        Err(e) => {
            eprintln!("⚠ Warning: Could not fetch metadata: {}", e);
            eprintln!("  Continuing with manual input...");
            // Return the URL but with default metadata
            Ok(Some((url, UrlMetadata::default())))
        }
    }
}

/// Parse URL into a Repo type
fn parse_url(url: &str) -> Result<Repo> {
    let normalized_url = url
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');

    if GITHUB_URL_REGEX.is_match(normalized_url) {
        let captures = GITHUB_URL_REGEX.captures(normalized_url).unwrap();
        Ok(Repo::Github(GithubRepo {
            owner: captures.get(1).unwrap().as_str().to_owned(),
            repo: captures.get(2).unwrap().as_str().to_owned(),
        }))
    } else if PYPI_URL_REGEX.is_match(normalized_url) {
        let captures = PYPI_URL_REGEX.captures(normalized_url).unwrap();
        Ok(Repo::Pypi(PypiRepo {
            project: captures.get(1).unwrap().as_str().to_owned(),
        }))
    } else {
        Err(anyhow!(
            "Invalid URL. Only github.com and pypi.org URLs are supported."
        ))
    }
}

/// Fetch versions from GitHub
fn fetch_github_versions(repo: &GithubRepo) -> Result<Vec<(String, String)>> {
    eprintln!("Fetching releases from GitHub...");
    let mut releases = fetch_github_release_info(repo);

    if releases.is_empty() {
        return Err(anyhow!("No releases found for this repository"));
    }

    // Filter out prereleases and sort
    releases = releases.into_iter().filter(|r| !r.prerelease).collect();
    releases.sort_by(|a, b| {
        VersionCompare::compare(&b.tag_name, &a.tag_name)
            .unwrap()
            .ord()
            .unwrap()
    });

    // Return (display_name, actual_version) tuples
    let versions: Vec<(String, String)> = releases
        .iter()
        .map(|r| {
            let parsed = VERSION_REGEX.captures(&r.tag_name).unwrap();
            let version = parsed.get(2).unwrap().as_str();
            let prefix = parsed.get(1).unwrap().as_str();
            let display = format!("{} (tag: {})", version, r.tag_name);
            (display, format!("{}|{}", prefix, version))
        })
        .collect();

    Ok(versions)
}

/// Fetch versions from PyPI
fn fetch_pypi_versions(repo: &PypiRepo) -> Result<Vec<(String, String)>> {
    eprintln!("Fetching releases from PyPI...");
    let response = fetch_pypi_project_info(repo);

    let mut versions: Vec<String> = response
        .releases
        .keys()
        .filter(|v| STABLE_RELEASE_REGEX.is_match(v))
        .cloned()
        .collect();

    if versions.is_empty() {
        return Err(anyhow!("No stable versions found on PyPI"));
    }

    // Sort versions
    versions.sort_by(|a, b| VersionCompare::compare(b, a).unwrap().ord().unwrap());

    Ok(versions.into_iter().map(|v| (v.clone(), v)).collect())
}

/// Prompt for version with auto-fetch from URL if available
pub fn prompt_version(url: Option<&str>, default: &str) -> Result<String> {
    let mut fetched_versions: Vec<(String, String)> = Vec::new();

    if let Some(url_str) = url {
        if let Ok(repo) = parse_url(url_str) {
            match repo {
                Repo::Github(gh_repo) => {
                    if let Ok(versions) = fetch_github_versions(&gh_repo) {
                        fetched_versions = versions;
                    }
                }
                Repo::Pypi(pypi_repo) => {
                    if let Ok(versions) = fetch_pypi_versions(&pypi_repo) {
                        fetched_versions = versions;
                    }
                }
            }
        }
    }

    if !fetched_versions.is_empty() {
        let mut options: Vec<String> = fetched_versions
            .iter()
            .map(|(display, _)| display.clone())
            .collect();
        options.push("Enter custom version".to_string());

        let selection = Select::new("Select version:", options)
            .with_starting_cursor(0)
            .prompt()?;

        if selection == "Enter custom version" {
            return prompt_version_manual(default);
        } else {
            // Find the selected version
            let selected = fetched_versions
                .iter()
                .find(|(display, _)| *display == selection)
                .unwrap();

            // Parse out prefix and version if GitHub (format: "prefix|version")
            if selected.1.contains('|') {
                let parts: Vec<&str> = selected.1.split('|').collect();
                return Ok(parts[1].to_string());
            } else {
                return Ok(selected.1.clone());
            }
        }
    }

    prompt_version_manual(default)
}

/// Prompt for version manually
fn prompt_version_manual(default: &str) -> Result<String> {
    let version = Text::new("Version:")
        .with_default(default)
        .prompt()?;
    Ok(version)
}

/// Prompt for package name
pub fn prompt_pname(default: &str) -> Result<String> {
    let pname = Text::new("Package name (pname):")
        .with_default(default)
        .with_help_message("The name attribute for the package")
        .prompt()?;
    Ok(pname)
}

/// Prompt for license
pub fn prompt_license(default: &str) -> Result<String> {
    let use_list = Confirm::new("Select from common licenses?")
        .with_default(true)
        .prompt()?;

    if use_list {
        let default_idx = COMMON_LICENSES
            .iter()
            .position(|&l| l == default)
            .unwrap_or(0);

        let selection = Select::new("License:", COMMON_LICENSES.to_vec())
            .with_starting_cursor(default_idx)
            .prompt()?;

        if selection == "Custom (enter manually)" {
            let custom = Text::new("Enter license:")
                .with_default(default)
                .prompt()?;
            Ok(custom)
        } else {
            Ok(selection.to_string())
        }
    } else {
        let license = Text::new("License:")
            .with_default(default)
            .prompt()?;
        Ok(license)
    }
}

/// Prompt for maintainer
pub fn prompt_maintainer(config_maintainer: Option<&str>) -> Result<String> {
    let default = config_maintainer.unwrap_or("");
    let maintainer = Text::new("Maintainer:")
        .with_default(default)
        .with_help_message("Your name or GitHub username")
        .prompt()?;
    Ok(maintainer)
}

/// Prompt for fetcher type
pub fn prompt_fetcher(default: Fetcher, _template: &Template) -> Result<Fetcher> {
    let options = vec![
        ("github", "fetchFromGitHub"),
        ("gitlab", "fetchFromGitLab"),
        ("pypi", "fetchPypi"),
        ("url", "fetchurl"),
        ("zip", "fetchzip"),
    ];

    let display_options: Vec<String> = options
        .iter()
        .map(|(name, desc)| format!("{:<10} - {}", name, desc))
        .collect();

    let default_idx = options
        .iter()
        .position(|(name, _)| {
            let fetcher_str = format!("{:?}", default);
            *name == fetcher_str.as_str()
        })
        .unwrap_or(0);

    let selection = Select::new("Select fetcher:", display_options)
        .with_starting_cursor(default_idx)
        .prompt()?;

    let fetcher_name = selection.split_whitespace().next().unwrap();

    Ok(match fetcher_name {
        "github" => Fetcher::github,
        "gitlab" => Fetcher::gitlab,
        "pypi" => Fetcher::pypi,
        "url" => Fetcher::url,
        "zip" => Fetcher::zip,
        _ => Fetcher::github,
    })
}

/// Prompt for output path
pub fn prompt_output_path(_template: &Template, default: &str) -> Result<String> {
    let path = Text::new("Output path:")
        .with_default(default)
        .with_help_message("File or directory where the expression will be written")
        .prompt()?;
    Ok(path)
}

/// Prompt for description
pub fn prompt_description(default: &str) -> Result<String> {
    let description = Text::new("Description:")
        .with_default(default)
        .with_help_message("Brief description of the package")
        .prompt()?;
    Ok(description)
}

/// Prompt for homepage
pub fn prompt_homepage(default: &str) -> Result<String> {
    let homepage = Text::new("Homepage:")
        .with_default(default)
        .with_help_message("Package homepage URL")
        .prompt()?;
    Ok(homepage)
}

/// Main interactive mode orchestrator
pub fn run_interactive_mode(
    initial_template: Option<Template>,
    user_config: Option<&UserConfig>,
) -> Result<InteractiveData> {
    println!("\n=== Interactive nix-template ===\n");

    // 1. Template type
    let template = prompt_template_type(initial_template)?;

    // 2. URL (optional) - fetch metadata immediately
    let url_with_metadata = prompt_url()?;

    // Extract metadata for defaults
    let metadata = url_with_metadata
        .as_ref()
        .map(|(_, meta)| meta.clone())
        .unwrap_or_default();

    // 3. Package name (pre-filled from URL if available)
    let pname = prompt_pname(&metadata.pname)?;

    // 4. Version (with auto-fetch if URL provided)
    let url_str = url_with_metadata.as_ref().map(|(url, _)| url.as_str());
    let version = prompt_version(url_str, "0.0.1")?;

    // 5. License (pre-filled from URL if available)
    let license = prompt_license(&metadata.license)?;

    // 6. Maintainer
    let maintainer_default = user_config.and_then(|c| c.maintainer.as_deref());
    let maintainer = prompt_maintainer(maintainer_default)?;

    // 7. Fetcher (auto-detected from URL or based on template)
    let default_fetcher = if url_with_metadata.is_some() {
        // Use fetcher from URL metadata
        metadata.fetcher
    } else if template == Template::python {
        Fetcher::pypi
    } else {
        Fetcher::github
    };
    let fetcher = prompt_fetcher(default_fetcher, &template)?;

    // 8. Description (pre-filled from URL if available)
    let description = prompt_description(&metadata.description)?;

    // 9. Homepage (pre-filled from URL if available)
    let default_homepage = if !metadata.homepage.is_empty() {
        &metadata.homepage
    } else {
        "https://github.com/CHANGE/CHANGE"
    };
    let homepage = prompt_homepage(default_homepage)?;

    // 10. Output path
    let default_path = match template {
        Template::mkshell => "shell.nix",
        Template::test => "test.nix",
        Template::flake => "flake.nix",
        _ => "default.nix",
    };
    let output_path = prompt_output_path(&template, default_path)?;

    // 11. Additional options
    let include_docs = Confirm::new("Include documentation links?")
        .with_default(false)
        .prompt()?;

    let include_meta = Confirm::new("Include meta section?")
        .with_default(true)
        .prompt()?;

    Ok(InteractiveData {
        template,
        pname,
        version,
        license,
        maintainer,
        fetcher,
        description,
        homepage,
        output_path,
        url: url_with_metadata.map(|(url, _)| url),
        include_documentation_links: include_docs,
        include_meta,
    })
}

/// Data collected from interactive prompts
#[derive(Debug)]
pub struct InteractiveData {
    pub template: Template,
    pub pname: String,
    pub version: String,
    pub license: String,
    pub maintainer: String,
    pub fetcher: Fetcher,
    pub description: String,
    pub homepage: String,
    pub output_path: String,
    pub url: Option<String>,
    pub include_documentation_links: bool,
    pub include_meta: bool,
}

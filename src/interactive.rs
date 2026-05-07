use crate::types::{Fetcher, GiteaRepo, GithubRepo, PypiRepo, Repo, Template, UserConfig};
use crate::url::{fetch_github_release_info, fetch_github_repo_info, fetch_pypi_project_info};
use anyhow::{anyhow, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use inquire::autocompletion::Replacement;
use inquire::{Autocomplete, Confirm, CustomUserError, Select, Text};
use regex::Regex;
use std::collections::HashMap;
use version_compare::VersionCompare;

/// Fuzzy scorer used by every interactive `Select`.
///
/// `inquire` 0.7's default scorer is a case-insensitive substring match
/// (`value.contains(input) ? Some(0) : None`), so typing e.g. `stnv`
/// does not match `stdenv`. We swap in `fuzzy-matcher`'s SkimV2
/// algorithm so users get the contiguous-skip matching they expect from
/// fzf-style pickers, with a real ranking score so closer matches sort
/// to the top.
///
/// `inquire`'s `Scorer<'a, T>` is `&'a dyn Fn(&str, &T, &str, usize) ->
/// Option<i64>`. `None` excludes the option; higher scores rank first.
fn fuzzy_score(input: &str, value: &str) -> Option<i64> {
    if input.trim().is_empty() {
        return Some(0);
    }
    SkimMatcherV2::default().fuzzy_match(value, input)
}

/// Scorer for `Select<String>` (most prompts).
fn fuzzy_scorer_string(input: &str, _opt: &String, value: &str, _idx: usize) -> Option<i64> {
    fuzzy_score(input, value)
}

/// Scorer for `Select<&str>` (e.g. the static COMMON_LICENSES list).
fn fuzzy_scorer_str(input: &str, _opt: &&str, value: &str, _idx: usize) -> Option<i64> {
    fuzzy_score(input, value)
}

/// `inquire::Autocomplete` implementation that powers Tab-completion on
/// free-text prompts using the same SkimV2 fuzzy matcher as the
/// `Select` scorers above.
///
/// As the user types, `get_suggestions` returns the suggestion list
/// re-ranked by fuzzy score (best match first). When the user presses
/// Tab, `get_completion` replaces their input with the highlighted
/// suggestion, or the top fuzzy match if no row is highlighted.
#[derive(Clone, Debug)]
struct FuzzyAutocomplete {
    suggestions: Vec<String>,
}

impl FuzzyAutocomplete {
    fn new<I, S>(items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            suggestions: items.into_iter().map(Into::into).collect(),
        }
    }

    /// Return suggestions ranked by fuzzy match against `input`.
    /// Empty input returns the full list in original order so the user
    /// can browse before typing.
    fn ranked(&self, input: &str) -> Vec<String> {
        if input.trim().is_empty() {
            return self.suggestions.clone();
        }
        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(i64, &String)> = self
            .suggestions
            .iter()
            .filter_map(|s| matcher.fuzzy_match(s, input).map(|score| (score, s)))
            .collect();
        // Higher score = better match; sort descending.
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, s)| s.clone()).collect()
    }
}

impl Autocomplete for FuzzyAutocomplete {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        Ok(self.ranked(input))
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        // Prefer whatever the user has highlighted in the dropdown.
        if let Some(s) = highlighted_suggestion {
            return Ok(Replacement::Some(s));
        }
        // Otherwise fall back to the top fuzzy match for what they
        // typed. This makes Tab behave like fzf's "accept best".
        Ok(self.ranked(input).into_iter().next().map_or(Replacement::None, Replacement::Some))
    }
}

lazy_static! {
    static ref GITHUB_URL_REGEX: Regex = {
        Regex::new("github.com/([^/]*)/([^/]*)/?").unwrap()
    };
    static ref PYPI_URL_REGEX: Regex = {
        Regex::new("pypi.org/project/([^/]*)/?").unwrap()
    };
    /// Hosts recognised as Gitea instances for interactive metadata extraction.
    static ref GITEA_HOSTS: Vec<&'static str> = vec!["codeberg.org", "gitea.com"];
    static ref GITEA_URL_REGEX: Regex = {
        Regex::new(r"(?P<domain>[^/]+)/(?P<owner>[^/]+)/(?P<repo>[^/]+)/?").unwrap()
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
}

impl Default for UrlMetadata {
    fn default() -> Self {
        Self {
            pname: "CHANGE".to_string(),
            license: "CHANGE".to_string(),
            description: "CHANGE".to_string(),
            homepage: "".to_string(),
            fetcher: Fetcher::github,
        }
    }
}

/// Best-effort heuristic: classify a PyPI project as application vs library
/// by inspecting its classifiers. Looks for `Environment :: Console`,
/// `Environment :: X11 Applications`, `Environment :: MacOS X`,
/// or `Environment :: Win32 (MS Windows)` which are typical for end-user
/// programs. Web-environment projects are intentionally excluded since
/// those are usually consumed as libraries (e.g. WSGI apps).
///
/// Note: Currently unused but kept for potential future use.
#[allow(dead_code)]
pub fn classifiers_suggest_application(classifiers: &[String]) -> bool {
    classifiers.iter().any(|c| {
        c.starts_with("Environment :: Console")
            || c.starts_with("Environment :: X11 Applications")
            || c.starts_with("Environment :: MacOS X")
            || c.starts_with("Environment :: Win32")
            || c.starts_with("Environment :: Handhelds")
    })
}

#[cfg(test)]
mod heuristic_tests {
    use super::classifiers_suggest_application;

    #[test]
    fn console_application_detected() {
        let classifiers = vec![
            "Programming Language :: Python".to_string(),
            "Environment :: Console".to_string(),
        ];
        assert!(classifiers_suggest_application(&classifiers));
    }

    #[test]
    fn pure_library_not_detected() {
        let classifiers = vec![
            "Programming Language :: Python".to_string(),
            "Topic :: Software Development :: Libraries".to_string(),
            "Intended Audience :: Developers".to_string(),
        ];
        assert!(!classifiers_suggest_application(&classifiers));
    }

    #[test]
    fn web_environment_not_classed_as_application() {
        // WSGI / web frameworks are typically consumed as libraries.
        let classifiers = vec!["Environment :: Web Environment".to_string()];
        assert!(!classifiers_suggest_application(&classifiers));
    }

    #[test]
    fn empty_classifiers() {
        assert!(!classifiers_suggest_application(&[]));
    }
}

#[cfg(test)]
mod fuzzy_tests {
    use super::fuzzy_score;

    #[test]
    fn empty_input_passes_everything() {
        // No filter input — every option should survive with a neutral
        // score so the original ordering is preserved.
        assert_eq!(fuzzy_score("", "stdenv"), Some(0));
        assert_eq!(fuzzy_score("   ", "anything"), Some(0));
    }

    #[test]
    fn substring_input_matches() {
        assert!(fuzzy_score("std", "stdenv").is_some());
        assert!(fuzzy_score("gpl", "lgpl21").is_some());
    }

    #[test]
    fn skipping_chars_still_matches() {
        // The whole point of swapping inquire's default substring scorer
        // for SkimV2: non-contiguous characters should still match.
        assert!(fuzzy_score("stnv", "stdenv").is_some());
        assert!(fuzzy_score("gthb", "github").is_some());
        assert!(fuzzy_score("lgp", "lgpl21").is_some());
    }

    #[test]
    fn unrelated_input_excluded() {
        assert!(fuzzy_score("xyzzy", "stdenv").is_none());
    }

    #[test]
    fn closer_matches_score_higher() {
        // SkimV2 ranks contiguous prefix matches above scattered ones.
        let exact = fuzzy_score("stdenv", "stdenv").unwrap();
        let scattered = fuzzy_score("sde", "stdenv").unwrap();
        assert!(exact > scattered, "exact={} scattered={}", exact, scattered);
    }
}

#[cfg(test)]
mod autocomplete_tests {
    use super::FuzzyAutocomplete;
    use inquire::autocompletion::Replacement;
    use inquire::Autocomplete;

    fn licenses() -> FuzzyAutocomplete {
        FuzzyAutocomplete::new(vec![
            "mit", "asl20", "gpl3", "gpl2", "lgpl21", "bsd3", "bsd2",
            "agpl3", "mpl20", "cc0", "unlicense",
        ])
    }

    #[test]
    fn empty_input_returns_full_list_in_order() {
        let mut ac = licenses();
        let suggestions = ac.get_suggestions("").unwrap();
        assert_eq!(suggestions.len(), 11);
        assert_eq!(suggestions[0], "mit");
        assert_eq!(suggestions[10], "unlicense");
    }

    #[test]
    fn input_filters_and_ranks() {
        let mut ac = licenses();
        let suggestions = ac.get_suggestions("gpl").unwrap();
        // Every gpl-flavoured license should appear; non-gpl entries
        // should be filtered out.
        assert!(suggestions.iter().any(|s| s == "gpl3"));
        assert!(suggestions.iter().any(|s| s == "gpl2"));
        assert!(suggestions.iter().any(|s| s == "lgpl21"));
        assert!(suggestions.iter().any(|s| s == "agpl3"));
        assert!(!suggestions.iter().any(|s| s == "mit"));
    }

    #[test]
    fn fuzzy_skipping_chars_matches() {
        let mut ac = licenses();
        // "ulcse" should fuzzy-match "unlicense"
        let suggestions = ac.get_suggestions("ulcse").unwrap();
        assert!(suggestions.iter().any(|s| s == "unlicense"));
    }

    #[test]
    fn tab_with_highlight_returns_highlighted() {
        let mut ac = licenses();
        let r = ac
            .get_completion("g", Some("gpl3".to_string()))
            .unwrap();
        assert_eq!(r, Replacement::Some("gpl3".to_string()));
    }

    #[test]
    fn tab_without_highlight_returns_top_match() {
        let mut ac = licenses();
        // No row highlighted; Tab should accept the best fuzzy match.
        let r = ac.get_completion("unlcs", None).unwrap();
        assert_eq!(r, Replacement::Some("unlicense".to_string()));
    }

    #[test]
    fn tab_with_no_match_returns_none() {
        let mut ac = licenses();
        let r = ac.get_completion("zzzzz", None).unwrap();
        assert_eq!(r, Replacement::None);
    }
}

/// Prompt for template type
pub fn prompt_template_type(default: Option<Template>) -> Result<Template> {
    let options = vec![
        ("stdenv", "Standard environment derivation"),
        ("stdenvNoCC", "Standard environment without a C compiler (fonts, data, scripts)"),
        ("python-package", "Python library with buildPythonPackage"),
        ("python-application", "Python application with buildPythonApplication"),
        ("rust", "Rust package with rustPlatform.buildRustPackage"),
        ("go", "Go package with buildGoModule"),
        ("npm", "Node.js package with buildNpmPackage"),
        ("pnpm", "Node.js package with pnpm (stdenv + fetchPnpmDeps)"),
        ("dotnet", ".NET package with buildDotnetModule"),
        ("ruby", "Ruby application with bundlerApp"),
        ("mkshell", "Development shell.nix"),
        ("module", "NixOS module"),
        ("test", "NixOS test"),
    ];

    let display_options: Vec<String> = options
        .iter()
        .map(|(name, desc)| format!("{:<20} - {}", name, desc))
        .collect();

    let default_idx = if let Some(template) = default {
        options.iter().position(|(name, _)| {
            let template_str = format!("{:?}", template);
            // Handle the snake_case to kebab-case mapping
            let kebab_name = template_str.replace('_', "-");
            *name == template_str.as_str() || *name == kebab_name.as_str()
        })
    } else {
        Some(0) // Default to stdenv
    };

    let selection = Select::new("Select template type:", display_options)
        .with_starting_cursor(default_idx.unwrap_or(0))
        .with_scorer(&fuzzy_scorer_string)
        .prompt()?;

    // Extract template name from the formatted string
    let template_name = selection.split_whitespace().next().unwrap();

    Ok(match template_name {
        "stdenv" => Template::stdenv(),
        "stdenvNoCC" => Template::stdenv_nocc(),
        "python-package" => Template::python_package(),
        "python-application" => Template::python_application(),
        "rust" => Template::rust(),
        "go" => Template::go(),
        "npm" => Template::npm(),
        "pnpm" => Template::pnpm(),
        "mkshell" => Template::Mkshell,
        "module" => Template::Module,
        "test" => Template::Test,
        _ => Template::stdenv(),
    })
}

/// Prompt the user to select from a list of auto-detected template candidates.
///
/// Called when multiple indicator files are found (e.g., both `Cargo.toml` and
/// `pyproject.toml`) and the user is in an interactive terminal.
pub fn prompt_template_from_candidates(
    candidates: &[crate::detect::Candidate],
) -> Result<Template> {
    let display_options: Vec<String> = candidates
        .iter()
        .map(|c| format!("{:<20} (detected from {})", c.template, c.reason))
        .collect();

    let selection = Select::new(
        "Multiple build systems detected. Select template:",
        display_options,
    )
    .with_scorer(&fuzzy_scorer_string)
    .prompt()?;

    // Match back to the candidate by finding which one matches the displayed string
    let selected_idx = candidates
        .iter()
        .position(|c| {
            selection.starts_with(&format!("{}", c.template))
        })
        .unwrap_or(0);

    Ok(candidates[selected_idx].template.clone())
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
                homepage: pypi_response.info.home_page.unwrap_or("CHANGE".to_string()),
                fetcher: Fetcher::pypi,
            })
        }
        Repo::Gitea(gitea_repo) => {
            // For interactive metadata extraction we don't perform the
            // network call here; full metadata is filled later via
            // `read_meta_from_url`. We surface only what we need to
            // pre-populate the prompts.
            eprintln!(
                "Detected Gitea URL ({}/{}/{}), full metadata will be fetched later.",
                gitea_repo.domain, gitea_repo.owner, gitea_repo.repo
            );
            let homepage = format!(
                "https://{}/{}/{}",
                gitea_repo.domain, gitea_repo.owner, gitea_repo.repo
            );
            Ok(UrlMetadata {
                pname: gitea_repo.repo.clone(),
                license: "CHANGE".to_string(),
                description: "CHANGE".to_string(),
                homepage,
                fetcher: Fetcher::gitea,
            })
        }
        Repo::Gitlab(gitlab_repo) => {
            // For interactive metadata extraction we don't perform the
            // network call here; full metadata is filled later via
            // `read_meta_from_url`. We surface only what we need to
            // pre-populate the prompts.
            eprintln!(
                "Detected GitLab URL ({}/{}), full metadata will be fetched later.",
                gitlab_repo.domain, gitlab_repo.project_path
            );
            let homepage = format!(
                "https://{}/{}",
                gitlab_repo.domain, gitlab_repo.project_path
            );
            Ok(UrlMetadata {
                pname: gitlab_repo.repo.clone(),
                license: "CHANGE".to_string(),
                description: "CHANGE".to_string(),
                homepage,
                fetcher: Fetcher::gitlab,
            })
        }
    }
}

/// Prompt for URL (GitHub or PyPI) and return URL with extracted metadata
pub fn prompt_url() -> Result<Option<(String, UrlMetadata)>> {
    let should_provide = Confirm::new("Do you want to fetch metadata from a URL (GitHub/PyPI/Gitea)?")
        .with_default(false)
        .prompt()?;

    if !should_provide {
        return Ok(None);
    }

    let url = Text::new("Enter GitHub, PyPI, or Gitea URL:")
        .with_help_message(
            "Tab to complete a host prefix. Examples: github.com/owner/repo, pypi.org/project/package, codeberg.org/owner/repo",
        )
        .with_autocomplete(FuzzyAutocomplete::new(vec![
            "https://github.com/",
            "https://pypi.org/project/",
            "https://codeberg.org/",
            "https://gitea.com/",
            "https://gitlab.com/",
        ]))
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
    } else if GITEA_HOSTS.iter().any(|h| normalized_url.starts_with(h)) {
        let captures = GITEA_URL_REGEX
            .captures(normalized_url)
            .ok_or_else(|| anyhow!("Invalid Gitea URL"))?;
        Ok(Repo::Gitea(GiteaRepo {
            domain: captures.name("domain").unwrap().as_str().to_owned(),
            owner: captures.name("owner").unwrap().as_str().to_owned(),
            repo: captures.name("repo").unwrap().as_str().to_owned(),
        }))
    } else {
        Err(anyhow!(
            "Invalid URL. Only github.com, pypi.org, and known Gitea hosts ({}) are supported.",
            GITEA_HOSTS.join(", "),
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
                Repo::Gitea(_) => {
                    // Version fetching for Gitea is handled later by
                    // `read_meta_from_url`; skip prompt-time enumeration.
                }
                Repo::Gitlab(_) => {
                    // Version fetching for GitLab is handled later by
                    // `read_meta_from_url`; skip prompt-time enumeration.
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
            .with_scorer(&fuzzy_scorer_string)
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
    loop {
        let pname = Text::new("Package name (pname):")
            .with_default(default)
            .with_help_message("The name attribute for the package")
            .prompt()?;

        // Validate length (max 255 characters for filesystem compatibility)
        if pname.len() > 255 {
            eprintln!("Error: Package name too long (max 255 characters, got {})", pname.len());
            continue;
        }

        // Validate characters (no control characters or path separators)
        if pname.chars().any(|c| c.is_control()) {
            eprintln!("Error: Package name contains control characters");
            continue;
        }

        if pname.contains('/') || pname.contains('\\') {
            eprintln!("Error: Package name cannot contain path separators (/ or \\)");
            continue;
        }

        // Validate not empty (after trimming)
        if pname.trim().is_empty() {
            eprintln!("Error: Package name cannot be empty");
            continue;
        }

        return Ok(pname);
    }
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
            .with_scorer(&fuzzy_scorer_str)
            .prompt()?;

        if selection == "Custom (enter manually)" {
            // Even on the "manual entry" path, let users tab-complete
            // against the common-license list — most "manual" entries
            // are still in this set, just not the top item.
            let custom = Text::new("Enter license:")
                .with_default(default)
                .with_autocomplete(FuzzyAutocomplete::new(COMMON_LICENSES.iter().copied()))
                .with_help_message("Tab to fuzzy-complete from common nixpkgs licenses")
                .prompt()?;
            Ok(custom)
        } else {
            Ok(selection.to_string())
        }
    } else {
        let license = Text::new("License:")
            .with_default(default)
            .with_autocomplete(FuzzyAutocomplete::new(COMMON_LICENSES.iter().copied()))
            .with_help_message("Tab to fuzzy-complete from common nixpkgs licenses")
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
        .with_scorer(&fuzzy_scorer_string)
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
    loop {
        let description = Text::new("Description:")
            .with_default(default)
            .with_help_message("Brief description of the package")
            .prompt()?;

        // Validate length (reasonable limit for descriptions)
        if description.len() > 1000 {
            eprintln!("Error: Description too long (max 1000 characters, got {})", description.len());
            continue;
        }

        // Allow empty descriptions (optional field)
        // No control character validation needed for descriptions as they're for human reading

        return Ok(description);
    }
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
    run_interactive_mode_with_defaults(
        initial_template,
        user_config,
        Vec::new(),      // detected_candidates
        None,            // default_pname
        None,            // inferred_deps
        false,           // is_local_init
    )
}

pub fn run_interactive_mode_with_defaults(
    initial_template: Option<Template>,
    user_config: Option<&UserConfig>,
    detected_candidates: Vec<crate::detect::Candidate>,
    default_pname: Option<String>,
    inferred_deps: Option<(Vec<String>, Vec<String>)>,
    is_local_init: bool,
) -> Result<InteractiveData> {
    println!("\n=== Interactive nix-template ===\n");

    // 1. Template type - use detected if available
    let template = if !detected_candidates.is_empty() && initial_template.is_none() {
        // Multiple candidates - prompt user to choose
        if detected_candidates.len() > 1 {
            prompt_template_from_candidates(&detected_candidates)?
        } else {
            // Single candidate - use it directly
            detected_candidates[0].template.clone()
        }
    } else {
        prompt_template_type(initial_template)?
    };

    // 2. URL (optional) - skip if we're in local init mode
    let url_with_metadata = if is_local_init {
        None
    } else {
        prompt_url()?
    };

    // Extract metadata for defaults
    let metadata = url_with_metadata
        .as_ref()
        .map(|(_, meta)| meta.clone())
        .unwrap_or_default();

    // 3. Package name (use defaults: default_pname from init mode, or URL metadata)
    let pname_default = default_pname
        .as_deref()
        .unwrap_or_else(|| &metadata.pname);
    let pname = prompt_pname(pname_default)?;

    // 4. Version (with auto-fetch if URL provided)
    let url_str = url_with_metadata.as_ref().map(|(url, _)| url.as_str());
    let version = prompt_version(url_str, "0.0.1")?;

    // 5. License (pre-filled from URL if available)
    let license = prompt_license(&metadata.license)?;

    // 6. Maintainer
    let maintainer_default = user_config.and_then(|c| c.maintainer.as_deref());
    let maintainer = prompt_maintainer(maintainer_default)?;

    // 7. Fetcher (auto-detected from URL, local for init mode, or based on template)
    let default_fetcher = if is_local_init {
        Fetcher::local
    } else if url_with_metadata.is_some() {
        // Use fetcher from URL metadata
        metadata.fetcher
    } else if template.is_python() {
        Fetcher::pypi
    } else {
        Fetcher::github
    };
    let fetcher = if is_local_init {
        Fetcher::local  // Force local fetcher in init mode
    } else {
        prompt_fetcher(default_fetcher, &template)?
    };

    // 8. Description (pre-filled from URL if available)
    let description = prompt_description(&metadata.description)?;

    // 9. Homepage (pre-filled from URL if available)
    let default_homepage = if !metadata.homepage.is_empty() {
        &metadata.homepage
    } else {
        "https://github.com/CHANGE/CHANGE"
    };
    let homepage = prompt_homepage(default_homepage)?;

    // 10. Output path (for init mode, use nix/pkgs/<pname>/package.nix)
    let default_path = if is_local_init {
        // In init mode, the structured layout will handle the path
        // Just use a placeholder that will be rewritten
        "nix/package.nix"
    } else {
        match template {
            Template::Mkshell => "shell.nix",
            Template::Test => "test.nix",
            _ => "default.nix",
        }
    };
    let output_path = prompt_output_path(&template, default_path)?;

    // 11. Additional options
    let include_docs = Confirm::new("Include documentation links?")
        .with_default(false)
        .prompt()?;

    let include_meta = Confirm::new("Include meta section?")
        .with_default(true)
        .prompt()?;

    // Hash prefetching is enabled by default for Rust/Go templates when we have a real source.
    // Ask if the user wants to skip it (opt-out behavior).
    let skip_vendor_hashes = if matches!(template, Template::Rust(_) | Template::Go(_))
        && url_with_metadata.is_some()
    {
        Confirm::new("Skip automatic cargoHash/vendorHash computation? (runs nix-build by default)")
            .with_default(false)
            .prompt()?
    } else {
        true // Skip if not rust/go or no URL
    };

    // Offer dependency inference for Rust and Go templates. For Rust we
    // parse Cargo.toml/Cargo.lock and look up known *-sys crates; for Go
    // we walk the source for `// #cgo` directives and translate
    // pkg-config / -l tokens into nixpkgs inputs. Default is on — users
    // can decline at the prompt.
    // In init mode, dependencies are already inferred, so just use true.
    let infer_deps = if is_local_init && inferred_deps.is_some() {
        true  // Already inferred in init mode
    } else if (template.is_rust() || template.is_go())
        && url_with_metadata.is_some()
    {
        let prompt_text = if template.is_rust() {
            "Infer system dependencies from Cargo.toml/Cargo.lock?"
        } else {
            "Infer system dependencies from CGO directives in *.go files?"
        };
        Confirm::new(prompt_text).with_default(true).prompt()?
    } else {
        false
    };

    // Ask about including prereleases if URL was provided
    let include_prereleases = if url_with_metadata.is_some() {
        Confirm::new("Include prerelease versions (alpha, beta, rc)?")
            .with_default(false)
            .prompt()?
    } else {
        false
    };

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
        skip_vendor_hashes,
        infer_deps,
        include_prereleases,
        preinferred_deps: inferred_deps,
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
    /// Whether to skip automatic computation of `cargoHash` (rust) / `vendorHash` (go).
    /// When false (default), nix-template runs nix-build against a probe expression
    /// with `lib.fakeHash` to compute the real hash.
    pub skip_vendor_hashes: bool,
    /// When the rust template is selected, infer system dependencies by
    /// inspecting the project's Cargo.toml.
    pub infer_deps: bool,
    /// Whether to include prerelease versions when fetching from GitLab or other forges.
    pub include_prereleases: bool,
    /// Pre-inferred dependencies (from init mode). (buildInputs, nativeBuildInputs)
    pub preinferred_deps: Option<(Vec<String>, Vec<String>)>,
}

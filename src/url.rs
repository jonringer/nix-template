use crate::types;
use crate::types::Repo::{Gitea, Github, Gitlab, Pypi};
use crate::types::{Template, FAKE_SRI_HASH};

use anyhow::anyhow;
use anyhow::Result;
use log::{debug, error};
use regex::Regex;
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::process::exit;
use std::process::Command;
use version_compare::VersionCompare;

lazy_static! {
    static ref GITHUB_URL_REGEX: Regex = {
        // e.g. github.com/jonringer/nix-template
        Regex::new("github.com/([^/]*)/([^/]*)/?").unwrap()
    };

    static ref PYPI_URL_REGEX: Regex = {
        // e.g. pypi.org/project/requests
        Regex::new("pypi.org/project/([^/]*)/?").unwrap()
    };

    static ref GITLAB_URL_REGEX: Regex = {
        // e.g. gitlab.com/gitlab-org/gitlab-foss or gitlab.com/org/subgroup/repo
        // Matches gitlab.com/ followed by any path (greedy, supports nested groups)
        Regex::new(r"gitlab\.com/(.+?)(?:\.git)?/?$").unwrap()
    };

    /// Hosts that we recognise as Gitea instances. Their REST APIs are
    /// compatible with each other (and largely with GitHub's), so we use a
    /// shared code path for fetching metadata.
    static ref GITEA_HOSTS: Vec<&'static str> = vec!["codeberg.org", "gitea.com"];

    static ref GITEA_URL_REGEX: Regex = {
        // e.g. codeberg.org/user/repo
        Regex::new(r"(?P<domain>[^/]+)/(?P<owner>[^/]+)/(?P<repo>[^/]+)/?").unwrap()
    };

    static ref VERSION_REGEX: Regex = {
        Regex::new("^([^0-9]*)(.+)").unwrap()
    };

    static ref STABLE_RELEASE_REGEX: Regex = {
        Regex::new(r"^([0-9.]*)+$").unwrap()
    };

    /// Matches the "got:" line emitted by `nix-build` when a fixed-output
    /// derivation has a hash mismatch. Examples:
    ///     got:    sha256-abcdef...=
    ///     got: sha256-abcdef...=
    static ref GOT_HASH_REGEX: Regex = {
        Regex::new(r"got:\s+(sha256-[A-Za-z0-9+/=]+)").unwrap()
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
        // TODO: add more licenses
        m.insert("Apache 2.0", "asl20");
        m.insert("Apache Software License", "asl20");
        m.insert("BSD-3-clause", "bsd3");
        m.insert("MIT License", "mit");
        m
    };

}

const LOG_TARGET: &str = "nix-template::url";

fn to_sri(hash: &str) -> String {
   let sha256_cmd = Command::new("nix")
     .args(&["hash", "to-sri", "--type", "sha256", "--experimental-features", "nix-command"])
     .arg(hash)
     .output();
   std::str::from_utf8(&sha256_cmd.expect("failed to run 'nix hash'").stdout)
     .unwrap()
     .trim()
     .to_owned()
}

/// Heuristic detection of prerelease tags for platforms that don't have
/// a dedicated prerelease field (e.g., GitLab).
///
/// Detects:
/// - Semver prereleases: 1.0.0-rc1, 1.0.0-alpha.1, etc. (contains - but not at end)
/// - Common prerelease patterns: -alpha, -beta, -rc, -pre (case-insensitive)
fn is_prerelease_tag(tag: &str) -> bool {
    // Check for semver prerelease format (version-prerelease+build)
    // e.g., "1.0.0-rc1" or "v2.0.0-alpha.1"
    if tag.contains('-') && !tag.ends_with('-') {
        let lowercase = tag.to_lowercase();
        // Only consider it a prerelease if it actually contains prerelease keywords
        // This avoids false positives for tags like "foo-bar-1.0"
        if lowercase.contains("alpha")
            || lowercase.contains("beta")
            || lowercase.contains("-rc")
            || lowercase.contains(".rc")
            || lowercase.contains("pre")
            || lowercase.contains("dev")
            || lowercase.contains("snapshot")
        {
            return true;
        }
    }
    false
}

/// Detect the forge platform (Gitea or GitLab) by probing version endpoints.
/// Returns Some("gitea") or Some("gitlab") if detected, None otherwise.
fn detect_forge_platform(domain: &str) -> Option<&'static str> {
    let client = Client::new();

    // Try Gitea first (API v1)
    let gitea_url = format!("https://{}/api/v1/version", domain);
    if let Ok(response) = client
        .get(&gitea_url)
        .header("User-Agent", "nix-template")
        .timeout(std::time::Duration::from_secs(5))
        .send()
    {
        if response.status().is_success() {
            return Some("gitea");
        }
    }

    // Try GitLab (API v4)
    let gitlab_url = format!("https://{}/api/v4/version", domain);
    if let Ok(response) = client
        .get(&gitlab_url)
        .header("User-Agent", "nix-template")
        .timeout(std::time::Duration::from_secs(5))
        .send()
    {
        if response.status().is_success() {
            return Some("gitlab");
        }
    }

    None
}

// This will just crazy the program, so no need to return a value
fn validate_and_parse_url(url: &str, original_url: &str) -> Result<types::Repo> {
    if url.starts_with("github.com") {
        if !GITHUB_URL_REGEX.is_match(url) {
            return Err(anyhow!(
                "Error: please provide a github url of shape 'github.com/<owner>/<repo>'"
            ));
        }

        let captures = GITHUB_URL_REGEX.captures(url).unwrap();

        return Ok(Github(types::GithubRepo {
            owner: captures.get(1).unwrap().as_str().to_owned(),
            repo: captures.get(2).unwrap().as_str().to_owned(),
        }));
    } else if url.starts_with("pypi.org") {
        if !PYPI_URL_REGEX.is_match(url) {
            return Err(anyhow!(
                "Error: please provide a pypi url of shape 'pypi.org/pypi/<repo>'"
            ));
        }

        let captures = PYPI_URL_REGEX.captures(url).unwrap();

        return Ok(Pypi(types::PypiRepo {
            project: captures.get(1).unwrap().as_str().to_owned(),
        }));
    } else if url.starts_with("gitlab.com") {
        if !GITLAB_URL_REGEX.is_match(url) {
            return Err(anyhow!(
                "Error: please provide a gitlab url of shape 'gitlab.com/<owner>/<repo>' or 'gitlab.com/<group>/<subgroup>/<repo>'"
            ));
        }

        let captures = GITLAB_URL_REGEX.captures(url).unwrap();
        let project_path = captures.get(1).unwrap().as_str().to_owned();

        // Split the project path to extract owner and repo
        let path_parts: Vec<&str> = project_path.split('/').collect();
        if path_parts.is_empty() {
            return Err(anyhow!("Invalid GitLab project path"));
        }

        let owner = path_parts[0].to_owned();
        let repo = path_parts[path_parts.len() - 1].to_owned();

        return Ok(Gitlab(types::GitlabRepo {
            domain: "gitlab.com".to_owned(),
            project_path,
            owner,
            repo,
        }));
    } else if GITEA_HOSTS.iter().any(|host| url.starts_with(host)) {
        let captures = GITEA_URL_REGEX.captures(url).ok_or_else(|| {
            anyhow!("Error: please provide a gitea url of shape '<domain>/<owner>/<repo>'")
        })?;

        return Ok(Gitea(types::GiteaRepo {
            domain: captures.name("domain").unwrap().as_str().to_owned(),
            owner: captures.name("owner").unwrap().as_str().to_owned(),
            repo: captures.name("repo").unwrap().as_str().to_owned(),
        }));
    } else {
        // Try to auto-detect the platform for unknown domains
        let captures = GITEA_URL_REGEX.captures(url).ok_or_else(|| {
            anyhow!("Error: please provide a url of shape '<domain>/<owner>/<repo>'")
        })?;

        let domain = captures.name("domain").unwrap().as_str();
        let owner = captures.name("owner").unwrap().as_str().to_owned();
        let repo = captures.name("repo").unwrap().as_str().to_owned();

        eprintln!("Detecting platform for {}...", domain);

        match detect_forge_platform(domain) {
            Some("gitea") => {
                eprintln!("Detected Gitea instance at {}", domain);
                Ok(Gitea(types::GiteaRepo {
                    domain: domain.to_owned(),
                    owner,
                    repo,
                }))
            }
            Some("gitlab") => {
                eprintln!("Detected GitLab instance at {}", domain);
                // For detected GitLab instances, construct the project_path
                let project_path = format!("{}/{}", owner, repo);
                Ok(Gitlab(types::GitlabRepo {
                    domain: domain.to_owned(),
                    project_path: project_path.clone(),
                    owner,
                    repo,
                }))
            }
            _ => Err(anyhow!(
                "{} is not a recognized forge platform. Could not detect Gitea or GitLab API at {}. Only github.com, gitlab.com, pypi.org, and self-hosted Gitea/GitLab instances are supported.",
                original_url,
                domain
            ))
        }
    }
}

fn get_json(request: reqwest::blocking::RequestBuilder) -> Result<String, reqwest::Error> {
    let response: reqwest::blocking::Response = request.send()?;
    response.error_for_status_ref()?;
    response.text()
}

pub fn fetch_pypi_project_info(pypi_repo: &types::PypiRepo) -> types::PypiResponse {
    let request_client = Client::new();
    let request = request_client
        .get(format!("https://pypi.io/pypi/{}/json", pypi_repo.project))
        .header("User-Agent", "reqwest")
        .header("Content", "application/json");

    let body = get_json(request).expect("Unable to get remote data.");
    let jd = &mut serde_json::Deserializer::from_str(&body);
    match serde_path_to_error::deserialize(jd) {
        Ok(s) => s,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Unable to parse response from pypi.io to json: {:?}", e
            );
            exit(1)
        }
    }
}

pub fn fetch_github_repo_info(repo: &types::GithubRepo) -> types::GhRepoResponse {
    let request_client = Client::new();
    let mut request = request_client
        .get(format!(
            "https://api.github.com/repos/{}/{}",
            repo.owner, repo.repo
        ))
        .header("User-Agent", "reqwest")
        .header("Accept", "application/vnd.github.v3+json");

    if let Ok(github_token) = std::env::var("GITHUB_TOKEN") {
        debug!(target: LOG_TARGET, "Using github token for github api request");
        request = request.header("Authorization", format!("token {}", github_token));
    }

    let body = get_json(request).expect("Unable to get remote data.");
    let jd = &mut serde_json::Deserializer::from_str(&body);
    match serde_path_to_error::deserialize(jd) {
        Ok(s) => s,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Unable to parse response from github to json: {:?}", e
            );
            exit(1)
        }
    }
}

pub fn fetch_github_release_info(repo: &types::GithubRepo) -> types::GhReleaseResponse {
    let request_client = Client::new();
    let mut request = request_client
        .get(format!(
            "https://api.github.com/repos/{}/{}/releases",
            repo.owner, repo.repo
        ))
        .header("User-Agent", "reqwest")
        .header("Accept", "application/vnd.github.v3+json");

    if let Ok(github_token) = std::env::var("GITHUB_TOKEN") {
        debug!(target: LOG_TARGET, "Using github token for polling events");
        request = request.header("Authorization", format!("token {}", github_token));
    }

    let body = get_json(request).expect("Unable to get remote data.");
    let jd = &mut serde_json::Deserializer::from_str(&body);
    match serde_path_to_error::deserialize(jd) {
        Ok(s) => s,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Unable to parse response from github to json: {:?}", e
            );
            exit(1)
        }
    }
}

pub fn fill_github_info(repo: &types::GithubRepo, info: &mut types::ExpressionInfo) {
    if info.pname == "CHANGE" {
        info.pname = repo.repo.to_string();
    }

    eprintln!("Determining latest release for {}", &repo.repo);
    let mut releases = fetch_github_release_info(&repo);
    if !releases.is_empty() {
        releases.sort_by(|a, b| {
            VersionCompare::compare(&b.tag_name, &a.tag_name)
                .unwrap()
                .ord()
                .unwrap()
        });
        releases = releases.into_iter().filter(|a| !a.prerelease).collect();
        let parsed_version = VERSION_REGEX
            .captures(&releases.first().unwrap().tag_name)
            .unwrap();
        info.version = parsed_version.get(2).unwrap().as_str().to_owned();
        info.tag_prefix = parsed_version.get(1).unwrap().as_str().to_owned();

        eprintln!("Determining sha256 for {}", &repo.repo);
        let sha256_cmd = Command::new("nix-prefetch-url")
            .args(&["--unpack", "--type", "sha256"])
            .arg(format!(
                "https://github.com/{}/{}/archive/refs/tags/{}{}.tar.gz",
                &repo.owner, &repo.repo, &info.tag_prefix, &info.version
            ))
            .output();
        let output = std::str::from_utf8(&sha256_cmd.unwrap().stdout)
            .unwrap()
            .trim()
            .to_owned();
        info.src_sha = to_sri(&output);
    } else {
        eprintln!(
            "No releases found for github.com/{}/{}",
            &repo.owner, &repo.repo
        );
    }

    let repo_info = fetch_github_repo_info(&repo);
    if repo_info.license.key != "other" {
        info.license = GITHUB_TO_NIXPKGS_LICENSE
            .get(&*repo_info.license.key)
            .unwrap_or(&"CHANGE")
            .to_string();
    }
    info.description = repo_info.description.unwrap_or("CHANGE".to_owned());

    if info.pname == "CHANGE" {
        info.pname = repo.repo.clone();
    }
    if info.owner == "CHANGE" {
        info.owner = repo.owner.clone();
    }
}

/// Populate `info` with metadata from a GitLab repository.
///
/// Uses GitLab's API v4 to fetch release and project information.
/// Supports nested groups (e.g., gitlab.com/org/subgroup/repo).
/// The project_path is URL-encoded for API calls.
pub fn fill_gitlab_info(repo: &types::GitlabRepo, info: &mut types::ExpressionInfo, include_prereleases: bool) {
    if info.pname == "CHANGE" {
        info.pname = repo.repo.to_string();
    }
    info.fetcher = types::Fetcher::gitlab;
    if info.owner == "CHANGE" {
        info.owner = repo.owner.clone();
    }

    let request_client = Client::new();

    // URL-encode the project path for API calls
    let project_path_encoded = urlencoding::encode(&repo.project_path);

    eprintln!("Determining latest release for {}", &repo.project_path);

    // Try the /permalink/latest endpoint first (GitLab 15.4+)
    let latest_url = format!(
        "https://{}/api/v4/projects/{}/releases/permalink/latest",
        repo.domain, project_path_encoded
    );

    let mut latest_request = request_client
        .get(&latest_url)
        .header("User-Agent", "nix-template")
        .header("Accept", "application/json");

    if let Ok(token) = std::env::var("GITLAB_TOKEN") {
        latest_request = latest_request.header("PRIVATE-TOKEN", token);
    }

    // Try latest endpoint first, fall back to list if not available
    let release_result = get_json(latest_request);

    let release_body = match release_result {
        Ok(body) => Some(body),
        Err(_) => {
            // Fallback: fetch all releases and find latest
            eprintln!("Latest release endpoint not available, fetching all releases...");
            let list_url = format!(
                "https://{}/api/v4/projects/{}/releases",
                repo.domain, project_path_encoded
            );

            let mut list_request = request_client
                .get(&list_url)
                .header("User-Agent", "nix-template")
                .header("Accept", "application/json");

            if let Ok(token) = std::env::var("GITLAB_TOKEN") {
                list_request = list_request.header("PRIVATE-TOKEN", token);
            }

            match get_json(list_request) {
                Ok(body) => Some(body),
                Err(e) => {
                    eprintln!("Warning: Could not fetch GitLab releases: {}", e);
                    None
                }
            }
        }
    };

    if let Some(body) = release_body {
        // Parse as list even if we got single release from /latest (wrap in array if needed)
        let releases_result: Result<types::GitlabReleaseResponse, _> = serde_json::from_str(&body);

        let mut releases = match releases_result {
            Ok(r) => r,
            Err(_) => {
                // Maybe it's a single release object from /latest endpoint
                let single: Result<types::GitlabReleaseElement, _> = serde_json::from_str(&body);
                match single {
                    Ok(r) => vec![r],
                    Err(e) => {
                        eprintln!("Warning: Could not parse GitLab release data: {}", e);
                        vec![]
                    }
                }
            }
        };

        if !releases.is_empty() {
            // Sort by released_at timestamp (most recent first)
            releases.sort_by(|a, b| b.released_at.cmp(&a.released_at));

            // Filter out prereleases using heuristic (unless --include-prereleases is set)
            if !include_prereleases {
                releases = releases
                    .into_iter()
                    .filter(|r| !is_prerelease_tag(&r.tag_name))
                    .collect();
            }

            if let Some(latest) = releases.first() {
                let parsed_version = VERSION_REGEX.captures(&latest.tag_name).unwrap();
                info.version = parsed_version.get(2).unwrap().as_str().to_owned();
                info.tag_prefix = parsed_version.get(1).unwrap().as_str().to_owned();

                eprintln!("Determining sha256 for {}", &repo.repo);

                // GitLab archive URL format: /-/archive/{tag}/{repo}-{tag}.tar.gz
                let archive_url = format!(
                    "https://{}/{}/-/archive/{}{}/{}-{}{}.tar.gz",
                    &repo.domain,
                    &repo.project_path,
                    &info.tag_prefix,
                    &info.version,
                    &repo.repo,
                    &info.tag_prefix,
                    &info.version
                );

                let sha256_cmd = Command::new("nix-prefetch-url")
                    .args(&["--unpack", "--type", "sha256"])
                    .arg(&archive_url)
                    .output();

                match sha256_cmd {
                    Ok(output) => {
                        if output.status.success() {
                            info.src_sha = to_sri(
                                String::from_utf8_lossy(&output.stdout)
                                    .to_string()
                                    .trim(),
                            );
                        } else {
                            eprintln!(
                                "Warning: nix-prefetch-url failed: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not run nix-prefetch-url: {}", e);
                    }
                }
            }
        } else {
            // No releases found, fallback to tags
            eprintln!("No releases found, trying tags API...");
            let tags_url = format!(
                "https://{}/api/v4/projects/{}/repository/tags",
                repo.domain, project_path_encoded
            );

            let mut tags_request = request_client
                .get(&tags_url)
                .header("User-Agent", "nix-template")
                .header("Accept", "application/json");

            if let Ok(token) = std::env::var("GITLAB_TOKEN") {
                tags_request = tags_request.header("PRIVATE-TOKEN", token);
            }

            match get_json(tags_request) {
                Ok(tags_body) => {
                    let tags_result: Result<types::GitlabTagsResponse, _> = serde_json::from_str(&tags_body);
                    if let Ok(mut tags) = tags_result {
                        if !tags.is_empty() {
                            // Sort by tag name using version comparison
                            tags.sort_by(|a, b| {
                                VersionCompare::compare(&b.name, &a.name)
                                    .unwrap()
                                    .ord()
                                    .unwrap()
                            });

                            // Filter out prereleases (unless --include-prereleases is set)
                            if !include_prereleases {
                                tags = tags
                                    .into_iter()
                                    .filter(|t| !is_prerelease_tag(&t.name))
                                    .collect();
                            }

                            if let Some(latest_tag) = tags.first() {
                                if let Some(captures) = VERSION_REGEX.captures(&latest_tag.name) {
                                    info.version = captures.get(2).unwrap().as_str().to_owned();
                                    info.tag_prefix = captures.get(1).unwrap().as_str().to_owned();

                                    eprintln!("Determining sha256 for {}", &repo.repo);

                                    let archive_url = format!(
                                        "https://{}/{}/-/archive/{}{}/{}-{}{}.tar.gz",
                                        &repo.domain,
                                        &repo.project_path,
                                        &info.tag_prefix,
                                        &info.version,
                                        &repo.repo,
                                        &info.tag_prefix,
                                        &info.version
                                    );

                                    let sha256_cmd = Command::new("nix-prefetch-url")
                                        .args(&["--unpack", "--type", "sha256"])
                                        .arg(&archive_url)
                                        .output();

                                    match sha256_cmd {
                                        Ok(output) => {
                                            if output.status.success() {
                                                info.src_sha = to_sri(
                                                    String::from_utf8_lossy(&output.stdout)
                                                        .to_string()
                                                        .trim(),
                                                );
                                            } else {
                                                eprintln!(
                                                    "Warning: nix-prefetch-url failed: {}",
                                                    String::from_utf8_lossy(&output.stderr)
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Warning: Could not run nix-prefetch-url: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Could not fetch GitLab tags: {}", e);
                }
            }
        }
    }

    // Fetch project metadata for description and license
    let project_url = format!(
        "https://{}/api/v4/projects/{}",
        repo.domain, project_path_encoded
    );

    let mut project_request = request_client
        .get(&project_url)
        .header("User-Agent", "nix-template")
        .header("Accept", "application/json");

    if let Ok(token) = std::env::var("GITLAB_TOKEN") {
        project_request = project_request.header("PRIVATE-TOKEN", token);
    }

    match get_json(project_request) {
        Ok(body) => {
            let project: Result<types::GitlabProjectResponse, _> = serde_json::from_str(&body);
            if let Ok(proj) = project {
                if let Some(desc) = proj.description {
                    info.description = desc;
                }
                info.homepage = proj.web_url;

                if let Some(license) = proj.license {
                    info.license = GITHUB_TO_NIXPKGS_LICENSE
                        .get(license.key.as_str())
                        .unwrap_or(&"CHANGE")
                        .to_string();
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Could not fetch GitLab project metadata: {}", e);
            info.homepage = format!("https://{}/{}", repo.domain, repo.project_path);
        }
    }
}

/// Populate `info` with metadata from a Gitea repository.
///
/// Gitea's REST API closely mirrors GitHub's: `/api/v1/repos/<owner>/<repo>`
/// returns repo metadata, and `/api/v1/repos/<owner>/<repo>/releases`
/// returns releases. We deserialize into the same response structs we use
/// for GitHub to keep the code path narrow.
///
/// Releases are not always present on a Gitea instance, so the function
/// gracefully degrades to leaving the version/hash placeholders alone if
/// the API call fails or returns no releases.
pub fn fill_gitea_info(repo: &types::GiteaRepo, info: &mut types::ExpressionInfo) {
    if info.pname == "CHANGE" {
        info.pname = repo.repo.to_string();
    }
    info.fetcher = types::Fetcher::gitea;
    info.domain = repo.domain.clone();
    if info.owner == "CHANGE" {
        info.owner = repo.owner.clone();
    }

    let request_client = Client::new();

    eprintln!("Determining latest release for {}/{}", &repo.owner, &repo.repo);
    let releases_url = format!(
        "https://{}/api/v1/repos/{}/{}/releases",
        repo.domain, repo.owner, repo.repo
    );
    let mut releases_request = request_client
        .get(&releases_url)
        .header("User-Agent", "reqwest")
        .header("Accept", "application/json");

    // Add GITEA_TOKEN if available (using Authorization header)
    if let Ok(token) = std::env::var("GITEA_TOKEN") {
        releases_request = releases_request.header("Authorization", format!("token {}", token));
    }

    match get_json(releases_request) {
        Ok(body) => {
            let parsed: Result<types::GhReleaseResponse, _> = serde_json::from_str(&body);
            match parsed {
                Ok(mut releases) if !releases.is_empty() => {
                    releases.sort_by(|a, b| {
                        VersionCompare::compare(&b.tag_name, &a.tag_name)
                            .unwrap()
                            .ord()
                            .unwrap()
                    });
                    releases = releases.into_iter().filter(|a| !a.prerelease).collect();
                    if let Some(latest) = releases.first() {
                        let parsed_version =
                            VERSION_REGEX.captures(&latest.tag_name).unwrap();
                        info.version = parsed_version.get(2).unwrap().as_str().to_owned();
                        info.tag_prefix =
                            parsed_version.get(1).unwrap().as_str().to_owned();

                        eprintln!("Determining sha256 for {}", &repo.repo);
                        // Gitea archive URL: <domain>/<owner>/<repo>/archive/<tag>.tar.gz
                        let archive_url = format!(
                            "https://{}/{}/{}/archive/{}{}.tar.gz",
                            &repo.domain,
                            &repo.owner,
                            &repo.repo,
                            &info.tag_prefix,
                            &info.version,
                        );
                        let sha256_cmd = Command::new("nix-prefetch-url")
                            .args(&["--unpack", "--type", "sha256"])
                            .arg(&archive_url)
                            .output();
                        if let Ok(out) = sha256_cmd {
                            let raw = std::str::from_utf8(&out.stdout)
                                .unwrap_or("")
                                .trim()
                                .to_owned();
                            if !raw.is_empty() {
                                info.src_sha = to_sri(&raw);
                            }
                        }
                    }
                }
                Ok(_) => eprintln!(
                    "No releases found for {}/{}/{}",
                    &repo.domain, &repo.owner, &repo.repo
                ),
                Err(e) => error!(
                    target: LOG_TARGET,
                    "Unable to parse Gitea releases response: {:?}", e
                ),
            }
        }
        Err(e) => eprintln!("Failed to fetch Gitea releases: {}", e),
    }

    // Repo metadata: description and homepage.
    let repo_url = format!(
        "https://{}/api/v1/repos/{}/{}",
        repo.domain, repo.owner, repo.repo
    );
    let repo_request = request_client
        .get(&repo_url)
        .header("User-Agent", "reqwest")
        .header("Accept", "application/json");

    if let Ok(body) = get_json(repo_request) {
        // Gitea's repo response has a `description` field; reuse the
        // GhRepoResponse deserializer where compatible.
        if let Ok(parsed) = serde_json::from_str::<types::GhRepoResponse>(&body) {
            if info.description == "CHANGE" {
                info.description = parsed
                    .description
                    .unwrap_or_else(|| "CHANGE".to_owned());
            }
        }
    }

    info.homepage = format!(
        "https://{}/{}/{}",
        repo.domain, repo.owner, repo.repo
    );
}

pub fn fill_pypi_info(pypi_repo: &types::PypiRepo, info: &mut types::ExpressionInfo) {

    eprintln!("Determining latest release for {}", &pypi_repo.project);
    let pypi_response = fetch_pypi_project_info(pypi_repo);
    if info.pname == "CHANGE" {
        info.pname = pypi_repo.project.clone();
    }

    let mut releases: Vec<String> = pypi_response
        .releases
        .keys()
        .map(|a| a.to_owned())
        .filter(|v| STABLE_RELEASE_REGEX.is_match(v))
        .collect();

    if !releases.is_empty() {
        releases.sort_by(|a, b| VersionCompare::compare(&b, &a).unwrap().ord().unwrap());

        let latest_version = releases.first().unwrap();

        let latest_release = pypi_response
            .releases
            .get(latest_version)
            .unwrap()
            .iter()
            .filter(|a| a.packagetype == "sdist")
            .next();

        info.version = latest_version.clone();
        info.homepage = pypi_response.info.home_page.unwrap_or("CHANGE".to_string());
        info.description = pypi_response.info.summary.trim_end_matches(".").to_string();

        info.license = PYPI_TO_NIXPKGS_LICENSE
            .get(&*pypi_response.info.license)
            .unwrap_or(&"CHANGE")
            .to_string();

        // Grab dependencies, filter out extras, normalize names
        debug!("Python dependencies before normalization: {:?}", &pypi_response.info.requires_dist);
        let mut dependencies: Vec<String> = pypi_response.info.requires_dist
            .unwrap_or_else(|| Vec::new())
            .into_iter()
            .filter(|s| !s.contains("extra =="))
            .map(|s| s.split(" ").next().unwrap()
                // Remove version information
                .chars().take_while( |&ch| ch != '!' && ch != '<' && ch != '>' && ch != '=').collect::<String>()
                // Normalize name to adhere to Nixpkgs conventions
                .replace(".", "-").replace("_", "-"))
            .collect();
        dependencies.sort();
        debug!("dependencies after normalization: {:?}", &dependencies);
        info.propagated_build_inputs = dependencies;

        match latest_release {
            Some(dist) => {
                info.fetcher = types::Fetcher::pypi;
                info.src_sha = to_sri(&dist.digests.sha256);
            }
            None => {
                eprintln!(
                    "Unable to find sdist for {}. Using default template",
                    &pypi_repo.project
                );
            }
        }
    } else {
        eprintln!(
            "No releases found for pypi.org/project/{}",
            &pypi_repo.project
        );
    }
}

/// Prefetch the `cargoHash` (for `rust` template) or `vendorHash` (for `go`
/// template) by performing a build with `lib.fakeHash` and parsing the
/// resulting hash mismatch from `nix-build`'s stderr.
///
/// The expression at `info` must already have a known `src_sha`. The function
/// renders the package expression to a temporary file, invokes `nix-build`
/// against it via `callPackage`, and extracts the SRI hash from the
/// "got:" line that nix prints on hash mismatch.
///
/// Returns `None` if the build did not produce a hash mismatch (e.g. nix is
/// not installed, the source failed to fetch, the hash placeholder was
/// already correct, etc.). Logs progress to stderr.
pub fn prefetch_dependency_hash(info: &types::ExpressionInfo) -> Option<String> {
    use std::io::Write;

    // Only Rust, Go, npm, and pnpm packages need dependency hash prefetching.
    match info.template {
        Template::rust | Template::go | Template::npm | Template::pnpm => (),
        _ => return None,
    }

    // We can't prefetch without a real source hash to feed the builder.
    if info.src_sha.is_empty()
        || info.src_sha.starts_with("0000000000000000000000000000000000000000000000000000")
    {
        eprintln!("Skipping hash prefetch: src_sha is not yet known");
        return None;
    }

    // Render the expression with a fake dependency hash so that nix
    // is forced to fetch the dependencies and emit a "got:" line.
    let probe_info = types::ExpressionInfo {
        pname: info.pname.clone(),
        version: info.version.clone(),
        license: info.license.clone(),
        maintainer: info.maintainer.clone(),
        fetcher: info.fetcher.clone(),
        template: info.template.clone(),
        path_to_write: std::path::PathBuf::new(),
        top_level_path: std::path::PathBuf::new(),
        // Documentation links and meta would clutter / break the probe expression
        // (e.g. `licenses.CHANGE` does not exist).
        include_documentation_links: false,
        include_meta: false,
        tag_prefix: info.tag_prefix.clone(),
        owner: info.owner.clone(),
        src_sha: info.src_sha.clone(),
        description: info.description.clone(),
        homepage: info.homepage.clone(),
        propagated_build_inputs: info.propagated_build_inputs.clone(),
        cargo_hash: FAKE_SRI_HASH.to_owned(),
        vendor_hash: FAKE_SRI_HASH.to_owned(),
        npm_deps_hash: FAKE_SRI_HASH.to_owned(),
        pnpm_deps_hash: FAKE_SRI_HASH.to_owned(),
        project_file: String::new(),
        domain: info.domain.clone(),
        // Probe expressions don't need to render the inferred deps;
        // we want a minimal expression that just exercises src + cargo.
        build_inputs: Vec::new(),
        native_build_inputs: Vec::new(),
    };

    let probe_expr = crate::expression::generate_expression(&probe_info);
    let probe_text = probe_info.format(&probe_expr);

    // Write to a temp file (deleted when the variable goes out of scope).
    let tmp_dir = match tempfile_dir() {
        Some(d) => d,
        None => {
            eprintln!("Skipping hash prefetch: unable to create temporary directory");
            return None;
        }
    };
    let probe_path = tmp_dir.join("probe.nix");
    {
        let mut f = match std::fs::File::create(&probe_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Skipping hash prefetch: failed to write probe expression: {}", e);
                return None;
            }
        };
        if let Err(e) = f.write_all(probe_text.as_bytes()) {
            eprintln!("Skipping hash prefetch: failed to write probe expression: {}", e);
            return None;
        }
    }

    let kind = match info.template {
        Template::rust => "cargoHash",
        Template::go => "vendorHash",
        Template::npm => "npmDepsHash",
        Template::pnpm => "pnpmDeps hash",
        _ => unreachable!(),
    };
    eprintln!("Prefetching {} for {} (this may take a while)...", kind, &info.pname);

    let output = Command::new("nix-build")
        .args(&["--no-out-link", "-E"])
        .arg(format!(
            "(import <nixpkgs> {{}}).callPackage {} {{}}",
            probe_path.display()
        ))
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Skipping hash prefetch: failed to invoke nix-build: {}", e);
            return None;
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    debug!(target: LOG_TARGET, "nix-build stderr: {}", stderr);

    let captured = GOT_HASH_REGEX
        .captures_iter(&stderr)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        // The first hash mismatch is the dependency hash (cargo deps / go
        // vendor tree); subsequent hashes (if any) belong to later phases.
        .next();

    match captured {
        Some(h) => {
            eprintln!("Determined {} = {}", kind, &h);
            Some(h)
        }
        None => {
            eprintln!(
                "Could not determine {} from nix-build output. The placeholder will remain.",
                kind
            );
            None
        }
    }
}

/// Create a unique temporary directory under `$TMPDIR` (or `/tmp`).
/// Returns `None` on failure. The directory is leaked (not deleted) by the
/// caller so the user can inspect it on failure.
fn tempfile_dir() -> Option<std::path::PathBuf> {
    let base = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let dir = base.join(format!("nix-template-prefetch-{}", nanos));
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Infer the .NET project file path by materializing the source and scanning for
/// .csproj, .fsproj, or .sln files. Returns the path relative to the source root,
/// or None if no project file is found or the source cannot be materialized.
pub fn infer_dotnet_project_file(info: &types::ExpressionInfo) -> Option<String> {
    use std::path::Path;

    eprintln!("Materialising source to detect .NET project file...");
    let source_path = match crate::source::materialise_source(info) {
        Some(p) => p,
        None => {
            debug!(target: LOG_TARGET, "failed to materialise source; cannot infer project file");
            return None;
        }
    };

    // Search for project files in order of preference:
    // 1. .csproj files (C#)
    // 2. .fsproj files (F#)
    // 3. .sln files (Solution)

    fn find_project_files(dir: &Path, extension: &str) -> Vec<std::path::PathBuf> {
        use std::fs;

        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some(extension) {
                    results.push(path);
                } else if path.is_dir() {
                    // Recursively search subdirectories (up to 3 levels deep)
                    results.extend(find_project_files(&path, extension));
                }
            }
        }
        results
    }

    // Try .csproj first
    let mut candidates = find_project_files(&source_path, "csproj");
    if candidates.is_empty() {
        // Try .fsproj
        candidates = find_project_files(&source_path, "fsproj");
    }
    if candidates.is_empty() {
        // Try .sln
        candidates = find_project_files(&source_path, "sln");
    }

    if candidates.is_empty() {
        eprintln!("No .NET project files (.csproj, .fsproj, .sln) found in source");
        return None;
    }

    // Prefer files in the root directory, then shortest path
    candidates.sort_by_key(|p| p.components().count());

    let chosen = &candidates[0];
    let relative = chosen.strip_prefix(&source_path).ok()?;
    let relative_str = relative.to_str()?;

    eprintln!("Detected project file: {}", relative_str);
    Some(relative_str.to_owned())
}

pub fn read_meta_from_url(url: &str, info: &mut types::ExpressionInfo, include_prereleases: bool) {
    let trimmed_url = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    match validate_and_parse_url(trimmed_url, url) {
        Ok(Github(repo)) => {
            fill_github_info(&repo, info);
        }
        Ok(Gitlab(repo)) => {
            fill_gitlab_info(&repo, info, include_prereleases);
        }
        Ok(Pypi(pypi_repo)) => {
            fill_pypi_info(&pypi_repo, info);
        }
        Ok(Gitea(gitea_repo)) => {
            fill_gitea_info(&gitea_repo, info);
        }
        Err(e) => {
            eprintln!("{}", e);
            exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use crate::types::Repo::{Github, Gitea};
    use crate::types::{GiteaRepo, GithubRepo};

    #[test]
    fn test_url_parse() {
        let repo = validate_and_parse_url(
            "github.com/jonringer/nix-template",
            "github.com/jonringer/nix-template",
        )
        .unwrap();

        assert_eq!(repo, Github(GithubRepo {
            owner: "jonringer".to_string(),
            repo: "nix-template".to_string()
        }));
    }

    #[test]
    fn test_codeberg_url_parse() {
        let repo = validate_and_parse_url(
            "codeberg.org/forgejo/forgejo",
            "codeberg.org/forgejo/forgejo",
        )
        .unwrap();

        assert_eq!(repo, Gitea(GiteaRepo {
            domain: "codeberg.org".to_string(),
            owner: "forgejo".to_string(),
            repo: "forgejo".to_string(),
        }));
    }

    #[test]
    fn test_gitea_com_url_parse() {
        let repo = validate_and_parse_url(
            "gitea.com/user/project",
            "gitea.com/user/project",
        )
        .unwrap();

        assert_eq!(repo, Gitea(GiteaRepo {
            domain: "gitea.com".to_string(),
            owner: "user".to_string(),
            repo: "project".to_string(),
        }));
    }

    #[test]
    fn test_version_regex() {
        let captures = VERSION_REGEX.captures("v0.1.0").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.get(1).unwrap().as_str(), "v");
        assert_eq!(captures.get(2).unwrap().as_str(), "0.1.0");

        let captures = VERSION_REGEX.captures("azure-cli-21.1.3").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.get(1).unwrap().as_str(), "azure-cli-");
        assert_eq!(captures.get(2).unwrap().as_str(), "21.1.3");

        let captures = VERSION_REGEX.captures("zfs-2.1.0-rc7").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.get(1).unwrap().as_str(), "zfs-");
        assert_eq!(captures.get(2).unwrap().as_str(), "2.1.0-rc7");
    }

    #[test]
    fn test_simple_version_regex() {
        let captures = VERSION_REGEX.captures("2.1.1").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.get(1).unwrap().as_str(), "");
        assert_eq!(captures.get(2).unwrap().as_str(), "2.1.1");
    }
}

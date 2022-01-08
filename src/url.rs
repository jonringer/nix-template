use crate::types;
use crate::types::Repo::{Github, Pypi};

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

    static ref VERSION_REGEX: Regex = {
        Regex::new("^([^0-9]*)(.+)").unwrap()
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
    } else {
        Err(anyhow!(
            "{} is not a supported url. Only github.com and pypi.org are supported currently",
            original_url
        ))
    }
}

fn get_json(request: reqwest::blocking::RequestBuilder) -> Result<String, reqwest::Error> {
    let response: reqwest::blocking::Response = request.send()?;
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
        info.src_sha = std::str::from_utf8(&sha256_cmd.unwrap().stdout)
            .unwrap()
            .trim()
            .to_owned();
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

pub fn fill_pypi_info(pypi_repo: &types::PypiRepo, info: &mut types::ExpressionInfo) {
    eprintln!("Determining latest release for {}", &pypi_repo.project);
    let pypi_response = fetch_pypi_project_info(pypi_repo);
    let mut releases: Vec<String> = pypi_response
        .releases
        .keys()
        .map(|a| a.to_owned())
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

        info.pname = pypi_repo.project.clone();
        info.version = latest_version.clone();
        info.homepage = pypi_response.info.home_page.clone();
        info.description = pypi_response.info.summary.trim_end_matches(".").to_string();

        info.license = PYPI_TO_NIXPKGS_LICENSE
            .get(&*pypi_response.info.license)
            .unwrap_or(&"CHANGE")
            .to_string();
        match latest_release {
            Some(dist) => {
                info.fetcher = types::Fetcher::pypi;
                info.src_sha = dist.digests.sha256.clone();
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

pub fn read_meta_from_url(url: &str, info: &mut types::ExpressionInfo) {
    let trimmed_url = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    match validate_and_parse_url(trimmed_url, url) {
        Ok(Github(repo)) => {
            fill_github_info(&repo, info);
        }
        Ok(Pypi(pypi_repo)) => {
            fill_pypi_info(&pypi_repo, info);
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
    use crate::types::Repo::Github;
    use crate::types::GithubRepo;

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

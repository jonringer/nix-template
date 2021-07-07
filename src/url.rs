use crate::types;
use regex::Regex;
use lazy_static;
use std::process::exit;
use reqwest::blocking::{Client,Response};
use serde::Deserialize;
use serde_json;
use log::{error,debug,info};
use anyhow::{Context, Result};
use anyhow::anyhow;
use version_compare::VersionCompare;
use std::collections::HashMap;
use std::process::Command;

lazy_static! {
    static ref GITHUB_URL_REGEX: Regex = {
        // e.g. github.com/jonringer/nix-template
        Regex::new("github.com/([^/]*)/([^/]*)/?").unwrap()
    };

    static ref VERSION_REGEX: Regex = {
        Regex::new("^([^0-9]*)(.+)").unwrap()
    };

    static ref GITHUB_TO_NIXPKGS_LICENSE: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("agpl-3.0", "agpl3");
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
}

const LOG_TARGET: &str = "nix-template::url";

// This will just crazy the program, so no need to return a value
fn validate_and_parse_url(url: &str, original_url: &str) -> Result<types::GithubRepo>  {
    if url.starts_with("github.com") {
        if !GITHUB_URL_REGEX.is_match(url) {
            return Err(anyhow!("Error: please provide a github url of shape 'github.com/<owner>/<repo>'"))
        }

        let captures = GITHUB_URL_REGEX.captures(url).unwrap();

        return Ok(types::GithubRepo {
            owner: captures.get(1).unwrap().as_str().to_owned(),
            repo: captures.get(2).unwrap().as_str().to_owned(),
        })
    } else {
        Err(anyhow!("{} is not a supported url. Only github.com is supported currently", original_url))
    }
}

fn get_json(request: reqwest::blocking::RequestBuilder) -> Result<String, reqwest::Error> {
    let response: reqwest::blocking::Response = request.send()?;
    response.text()
}

pub fn fetch_github_repo_info(repo: &types::GithubRepo) -> types::GhRepoResponse {
    let request_client = Client::new();
    let mut request = request_client
        .get(format!("https://api.github.com/repos/{}/{}", repo.owner, repo.repo))
        .header("User-Agent", "reqwest")
        .header("Accept", "application/vnd.github.v3+json");
        
    if let Ok(github_token) = std::env::var("GITHUB_TOKEN") {
        debug!(target: LOG_TARGET, "Using github token for polling events");
        request = request.header("Authorization", format!("token {}", github_token));
    }

    let body = get_json(request).expect("Unable to get remote data.");
    match serde_json::from_str(&body) {
        Ok(s) => s,
        Err(e) => {
            error!(target: LOG_TARGET, "Unable to parse response from github to json: {:?}", e);
            exit(1)
        }
    }
}

pub fn fetch_github_release_info(repo: &types::GithubRepo) -> types::GhReleaseResponse {
    let request_client = Client::new();
    let mut request = request_client
        .get(format!("https://api.github.com/repos/{}/{}/releases", repo.owner, repo.repo))
        .header("User-Agent", "reqwest")
        .header("Accept", "application/vnd.github.v3+json");
        
    if let Ok(github_token) = std::env::var("GITHUB_TOKEN") {
        debug!(target: LOG_TARGET, "Using github token for polling events");
        request = request.header("Authorization", format!("token {}", github_token));
    }

    let body = get_json(request).expect("Unable to get remote data.");
    match serde_json::from_str(&body) {
        Ok(s) => s,
        Err(e) => {
            error!(target: LOG_TARGET, "Unable to parse response from github to json: {:?}", e);
            exit(1)
        }
    }
}
pub fn read_meta_from_url(url: &str, info: &mut types::ExpressionInfo) {
    let trimmed_url = url.trim_start_matches("http://").trim_start_matches("https://");

    match validate_and_parse_url(trimmed_url, url) {
        Ok(repo) => {
            eprintln!("Determining latest release for {}", &repo.repo);
            let mut releases = fetch_github_release_info(&repo);
            if releases.len() > 0 {
                releases.sort_by(|a,b| VersionCompare::compare(&b.tag_name, &a.tag_name).unwrap().ord().unwrap());
                releases = releases.into_iter().filter(|a| !a.prerelease).collect();
                let parsed_version = VERSION_REGEX.captures(&releases.first().unwrap().tag_name).unwrap();
                info.version = parsed_version.get(2).unwrap().as_str().to_owned();
                info.tag_prefix = parsed_version.get(1).unwrap().as_str().to_owned();

                eprintln!("Determining sha256 for {}", &repo.repo);
                let sha256_cmd = Command::new("nix-prefetch-url")
                    .args(&["--unpack", "--type", "sha256"])
                    .arg(format!("https://github.com/{}/{}/archive/refs/tags/{}{}.tar.gz",
                        &repo.owner, &repo.repo, &info.tag_prefix, &info.version))
                    .output();
                info.src_sha = std::str::from_utf8(&sha256_cmd.unwrap().stdout).unwrap().trim().to_owned();

            } else {
                eprintln!("No releases found for {}", &url);
            }

            let repo_info = fetch_github_repo_info(&repo);
            if repo_info.license.key != "other" {
                info.license = GITHUB_TO_NIXPKGS_LICENSE.get(&*repo_info.license.key).unwrap_or(&"CHANGE").to_string();
            }
            info.description = repo_info.description;

            if info.pname == "CHANGE" {
                info.pname = repo.repo;
            }
            if info.owner == "CHANGE" {
                info.owner = repo.owner;
            }
        },
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

    #[test]
    fn test_url_parse() {
        let repo = validate_and_parse_url("github.com/jonringer/nix-template", "github.com/jonringer/nix-template").unwrap();

        assert_eq!(repo.owner, "jonringer");
        assert_eq!(repo.repo, "nix-template");
    }

    #[test]
    fn test_version_regix() {
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
    fn test_simple_version_regix() {
        let captures = VERSION_REGEX.captures("2.1.1").unwrap();
        assert_eq!(captures.len(), 3);
        assert_eq!(captures.get(1).unwrap().as_str(), "");
        assert_eq!(captures.get(2).unwrap().as_str(), "2.1.1");
    }
}
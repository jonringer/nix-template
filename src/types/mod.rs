use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use clap::arg_enum;
use regex::{Captures, Regex};

pub mod gh_release_response;
pub mod gh_repo_response;

pub use gh_release_response::*;
pub use gh_repo_response::*;

lazy_static! {
    static ref DOCUMENTATION_LINKS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert(
            "buildDependencies",
            "https://nixos.org/nixpkgs/manual/#ssec-stdenv-dependencies\n  ",
        );
        m.insert(
            "fetcher",
            "https://nixos.org/nixpkgs/manual/#chap-pkgs-fetchers\n  ",
        );
        m.insert("meta", "https://nixos.org/nixpkgs/manual/#chap-meta\n  ");
        m.insert(
            "stdenvMkDerivation",
            "https://nixos.org/nixpkgs/manual/#sec-using-stdenv\n  ",
        );
        m
    };
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug,PartialEq)]
    pub enum Template {
        flake,
        stdenv,
        python,
        mkshell,
        go,
        rust,
        qt,
        test,
    }
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug)]
    pub enum Fetcher {
        github,
        gitlab,
        url,
        zip,
        pypi,
    }
}

#[derive(Debug)]
pub struct GithubRepo {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug)]
pub struct ExpressionInfo {
    pub pname: String,
    pub version: String,
    pub license: String,
    pub maintainer: String,
    pub fetcher: Fetcher,
    pub template: Template,
    pub path_to_write: std::path::PathBuf,
    pub top_level_path: std::path::PathBuf,
    pub include_documentation_links: bool,
    pub include_meta: bool,
    pub tag_prefix: String,
    pub owner: String,
    pub src_sha: String,
    pub description: String,
}

impl ExpressionInfo {
    pub fn format(&self, s: &str) -> String {
        let rev: String = if self.tag_prefix.is_empty() {
            "version".to_owned()
        } else {
            format!(r"{}${{version}}", &self.tag_prefix)
        };

        let result = s
            .to_owned()
            .replace("@pname@", &self.pname)
            .replace("@pname-import-check@", &self.pname.replace("-", ".")) // used for pythonImportsCheck, "azure-mgmt" -> "azure.mgmt"
            .replace("@version@", &self.version)
            .replace("@owner@", &self.owner)
            .replace("@rev@", &rev)
            .replace("@src_sha@", &self.src_sha)
            .replace("@description@", &self.description)
            .replace("@license@", &self.license)
            .replace("@maintainer@", &self.maintainer);

        if self.include_documentation_links {
            Self::insert_documentation_links(result)
        } else {
            Regex::new(r"@doc:.*@")
                .unwrap()
                .replace_all(&result, "")
                .to_string()
        }
    }

    fn insert_documentation_links(s: String) -> String {
        let re = Regex::new(r"@doc:(\w+)@").unwrap();

        re.replace_all(&s, |caps: &Captures| {
            let key = &caps[1];
            format!(
                "# See the guide for more information: {}",
                DOCUMENTATION_LINKS.get(key).unwrap_or(&"").to_string()
            )
        })
        .to_string()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserConfig {
    pub maintainer: Option<String>,
    pub nixpkgs_root: Option<String>,
}

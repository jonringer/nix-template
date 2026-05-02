use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use clap::arg_enum;
use regex::{Captures, Regex};

pub mod gh_release_response;
pub mod gh_repo_response;
pub mod pypi;

pub use gh_release_response::*;
pub use gh_repo_response::*;
pub use pypi::*;

lazy_static! {
    static ref DOCUMENTATION_LINKS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Updated to use nix-book as primary reference
        m.insert(
            "buildDependencies",
            "https://ekala-project.github.io/nix-book/ch06-04-build-dependencies.html\n  ",
        );
        m.insert(
            "stdenvMkDerivation",
            "https://ekala-project.github.io/nix-book/ch06-02-stdenv.html\n  ",
        );
        // Fallback to nixpkgs manual (not yet in nix-book)
        m.insert(
            "fetcher",
            "https://nixos.org/nixpkgs/manual/#chap-pkgs-fetchers\n  ",
        );
        m.insert("meta", "https://nixos.org/nixpkgs/manual/#chap-meta\n  ");
        // Template-specific documentation
        m.insert(
            "pythonImportsCheck",
            "https://ekala-project.github.io/nix-book/ch07-04-python.html\n  ",
        );
        m.insert(
            "pythonFormat",
            "https://ekala-project.github.io/nix-book/ch07-04-python.html\n  ",
        );
        m.insert(
            "cargoHash",
            "https://ekala-project.github.io/nix-book/ch07-05-rust.html\n  ",
        );
        m.insert(
            "vendorHash",
            "https://ekala-project.github.io/nix-book/ch07-06-go.html\n  ",
        );
        m.insert(
            "goSubPackages",
            "https://ekala-project.github.io/nix-book/ch07-06-go.html\n  ",
        );
        // Build phases and NixOS modules
        m.insert(
            "buildPhases",
            "https://ekala-project.github.io/nix-book/ch06-03-phases.html\n  ",
        );
        m.insert(
            "nixosModules",
            "https://ekala-project.github.io/nix-book/ch09-00-nixos-modules.html\n  ",
        );
        m
    };
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq, Clone)]
    pub enum Template {
        stdenv,
        python,
        module,
        mkshell,
        go,
        rust,
        qt,
        test,
    }
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone)]
    pub enum Fetcher {
        github,
        gitlab,
        url,
        zip,
        pypi,
    }
}

#[derive(Debug, PartialEq)]
pub enum Repo {
    Pypi(PypiRepo),
    Github(GithubRepo),
}

#[derive(Debug, PartialEq)]
pub struct PypiRepo {
    pub project: String,
}

#[derive(Debug, PartialEq)]
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
    pub homepage: String,
    pub propagated_build_inputs: Vec<String>,
    /// SRI hash of the cargo dependencies (used for `rust` template).
    /// Defaults to `lib.fakeHash` when unknown.
    pub cargo_hash: String,
    /// SRI hash of the Go module vendor tree (used for `go` template).
    /// Defaults to `lib.fakeHash` when unknown.
    pub vendor_hash: String,
}

/// Default SRI placeholder used by `lib.fakeHash` in nixpkgs.
pub const FAKE_SRI_HASH: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

impl ExpressionInfo {
    pub fn format(&self, s: &str) -> String {
        let rev: String = if self.tag_prefix.is_empty() {
            "version".to_owned()
        } else {
            format!(r#""{}${{version}}""#, &self.tag_prefix)
        };

        fn format_inputs(inputs: &Vec<String>) -> String {
            if inputs.is_empty() {
                "".to_owned()
            } else {
                let body = inputs.join("\n    ");
                format!("\n    {}\n ", body)
            }
        }

        let result = s
            .to_owned()
            .replace("@pname@", &self.pname)
            .replace("@pname-import-check@", &self.pname.replace("-", ".")) // used for pythonImportsCheck, "azure-mgmt" -> "azure.mgmt"
            .replace("@version@", &self.version)
            .replace("@owner@", &self.owner)
            .replace("@rev@", &rev)
            .replace("@src_sha@", &self.src_sha)
            .replace("@cargo_hash@", &self.cargo_hash)
            .replace("@vendor_hash@", &self.vendor_hash)
            .replace("@description@", &self.description)
            .replace("@homepage@", &self.homepage)
            .replace("@license@", &self.license)
            .replace("@maintainer@", &self.maintainer)
            .replace("@propagated_build_inputs@", &format_inputs(&self.propagated_build_inputs));

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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use clap::arg_enum;
use regex::{Captures, Regex};

pub mod gh_release_response;
pub mod gh_repo_response;
pub mod gitlab_response;
pub mod pypi;

pub use gh_release_response::*;
pub use gh_repo_response::*;
pub use gitlab_response::*;
pub use pypi::*;

// Re-export template types from the templates module for backward compatibility
pub use crate::templates::types::*;

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
        // stdenvNoCC shares the same chapter as stdenv; the documentation
        // discusses both variants together.
        m.insert(
            "stdenvNoCCMkDerivation",
            "https://ekala-project.github.io/nix-book/ch06-02-stdenv.html\n  ",
        );
        // Fallback to nixpkgs manual (not yet in nix-book)
        m.insert(
            "fetcher",
            "https://nixos.org/nixpkgs/manual/#chap-pkgs-fetchers\n  ",
        );
        m.insert(
            "fetcherPypi",
            "https://nixos.org/nixpkgs/manual/#chap-pkgs-fetchers\n  # NOTE: fetchPypi is discouraged in nixpkgs; prefer fetching from the original source (GitHub, GitLab, etc.)\n  ",
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

    // Regex for removing documentation placeholder markers
    static ref DOC_REGEX: Regex = Regex::new(r"@doc:.*@").unwrap();

    // Regex for extracting documentation link keys
    static ref DOC_LINKS_REGEX: Regex = Regex::new(r"@doc:(\w+)@").unwrap();
}

// Template types have been moved to the template module.
// The new hierarchical Template enum is now re-exported above.

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone, PartialEq)]
    pub enum Fetcher {
        github,
        gitlab,
        gitea,
        url,
        zip,
        pypi,
        local,
    }
}

#[derive(Debug, PartialEq)]
pub enum Repo {
    Pypi(PypiRepo),
    Github(GithubRepo),
    Gitlab(GitlabRepo),
    Gitea(GiteaRepo),
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

#[derive(Debug, PartialEq)]
pub struct GitlabRepo {
    pub domain: String,
    /// Full project path including nested groups (e.g., "gitlab-org/frontend/design-system")
    pub project_path: String,
    /// Owner (first component of project_path, e.g., "gitlab-org")
    pub owner: String,
    /// Repository name (last component of project_path, e.g., "design-system")
    pub repo: String,
}

#[derive(Debug, PartialEq)]
pub struct GiteaRepo {
    pub domain: String,
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
    /// SRI hash of the npm dependencies (used for `npm` template).
    /// Defaults to `lib.fakeHash` when unknown.
    pub npm_deps_hash: String,
    /// SRI hash of the pnpm dependencies (used for `pnpm` template).
    /// Defaults to `lib.fakeHash` when unknown.
    pub pnpm_deps_hash: String,
    /// Path to the .NET project file (used for `dotnet` template).
    /// Typically a .csproj, .fsproj, or .sln file relative to src root.
    pub project_file: String,
    /// Domain of the Gitea instance (used by the `gitea` fetcher), e.g.
    /// "codeberg.org" or "gitea.com". Empty for non-Gitea fetchers.
    pub domain: String,
    /// Inferred system libraries that need to be linked at build time
    /// (rendered into `buildInputs` for the `rust` template).
    pub build_inputs: Vec<String>,
    /// Inferred build-time tools (rendered into `nativeBuildInputs` for
    /// the `rust` template). Common entries: `pkg-config`, `cmake`.
    pub native_build_inputs: Vec<String>,
    /// When true, the Rust template renders `cargoLock.lockFile = ./Cargo.lock;`
    /// instead of `cargoHash = "...";`. Automatically set in local mode.
    pub use_cargo_lock_file: bool,
    /// Git dependency keys (`"name-version"`) from `Cargo.lock` that need
    /// `cargoLock.outputHashes` entries. Only populated when
    /// `use_cargo_lock_file` is true.
    pub cargo_lock_git_deps: Vec<String>,
    /// Go module path from `go.mod` (e.g. `github.com/user/repo`).
    /// Used to suggest `ldflags` for version embedding. Empty when unknown.
    pub go_module_path: String,
    /// Python build system format, detected from pyproject.toml or defaulted.
    /// One of: "setuptools", "pyproject", "flit", "poetry", "hatchling".
    pub python_format: String,
    /// SRI hash of the Maven dependencies (used for `maven` template).
    /// Defaults to `lib.fakeHash` when unknown.
    pub mvn_hash: String,
}

/// Default SRI placeholder used by `lib.fakeHash` in nixpkgs.
pub const FAKE_SRI_HASH: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// Sentinel value for `vendor_hash` that renders as `vendorHash = null;`
/// (without quotes) in the Go template. Used when a local project has a
/// committed `vendor/` directory.
pub const VENDOR_HASH_NULL: &str = "null";

impl ExpressionInfo {
    pub fn format(&self, s: &str) -> String {
        let rev: String = if self.tag_prefix.is_empty() {
            "finalAttrs.version".to_owned()
        } else {
            format!(r#""{}${{finalAttrs.version}}""#, &self.tag_prefix)
        };

        fn format_inputs(inputs: &Vec<String>) -> String {
            if inputs.is_empty() {
                "".to_owned()
            } else {
                // Deduplicate and sort inputs
                let unique: std::collections::BTreeSet<_> = inputs.iter().collect();
                let sorted: Vec<&String> = unique.into_iter().collect();
                let body = sorted
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n    ");
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
            .replace("@npm_deps_hash@", &self.npm_deps_hash)
            .replace("@pnpm_deps_hash@", &self.pnpm_deps_hash)
            .replace("@mvn_hash@", &self.mvn_hash)
            .replace("@project_file@", &self.project_file)
            .replace("@domain@", &self.domain)
            .replace("@description@", &self.description)
            .replace("@homepage@", &self.homepage)
            .replace("@license@", &self.license)
            .replace("@maintainer@", &self.maintainer)
            .replace(
                "@propagated_build_inputs@",
                &format_inputs(&self.propagated_build_inputs),
            )
            .replace("@build_inputs@", &format_inputs(&self.build_inputs))
            .replace(
                "@native_build_inputs@",
                &format_inputs(&self.native_build_inputs),
            )
            .replace("@python_format@", &self.python_format);

        if self.include_documentation_links {
            Self::insert_documentation_links(result)
        } else {
            DOC_REGEX.replace_all(&result, "").to_string()
        }
    }

    fn insert_documentation_links(s: String) -> String {
        DOC_LINKS_REGEX
            .replace_all(&s, |caps: &Captures| {
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

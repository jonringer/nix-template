//! Hierarchical template type system for nix-template.
//!
//! This module defines a tree-like structure for templates, where each
//! language or framework has its own configuration struct containing
//! variant-specific settings (e.g., Python has package/application variants
//! and multiple build formats; Rust has different lock file strategies).

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Top-level template type with hierarchical variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Template {
    /// Auto-detect template from source tree (requires --from-url)
    Auto,
    /// stdenv.mkDerivation or stdenvNoCC.mkDerivation
    Stdenv(StdenvVariant),
    /// Python package or application with build format
    Python(PythonConfig),
    /// Rust package with lock file strategy
    Rust(RustConfig),
    /// Go module with optional module path
    Go(GoConfig),
    /// Node.js package (npm or pnpm)
    Node(NodeConfig),
    /// .NET package (buildDotnetModule)
    Dotnet,
    /// Ruby application (bundlerApp)
    Ruby,
    /// Development shell (mkShell)
    Mkshell,
    /// NixOS module
    Module,
    /// Test template
    Test,
}

/// Stdenv variants: default (with CC) or NoCC (compiler-less).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StdenvVariant {
    /// stdenv.mkDerivation (includes compiler toolchain)
    Default,
    /// stdenvNoCC.mkDerivation (pure data, fonts, scripts)
    NoCC,
}

/// Python template configuration: variant (package vs application) × format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PythonConfig {
    pub variant: PythonVariant,
    pub format: PythonFormat,
}

/// Python package variant: library or application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PythonVariant {
    /// buildPythonPackage (library, reusable package)
    Package,
    /// buildPythonApplication (CLI tool, standalone app)
    Application,
}

/// Python build system format (detected from pyproject.toml).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PythonFormat {
    /// format = "setuptools" (legacy setup.py)
    Setuptools,
    /// format = "pyproject" (PEP 517/518, generic)
    Pyproject,
    /// format = "flit" (flit_core backend)
    Flit,
    /// format = "poetry" (poetry-core backend)
    Poetry,
    /// format = "hatchling" (hatchling backend)
    Hatchling,
}

impl PythonFormat {
    /// Convert to nixpkgs `format` attribute value.
    pub fn as_str(&self) -> &'static str {
        match self {
            PythonFormat::Setuptools => "setuptools",
            PythonFormat::Pyproject => "pyproject",
            PythonFormat::Flit => "flit",
            PythonFormat::Poetry => "poetry",
            PythonFormat::Hatchling => "hatchling",
        }
    }

    /// Parse from string (case-insensitive).
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "setuptools" => PythonFormat::Setuptools,
            "pyproject" => PythonFormat::Pyproject,
            "flit" => PythonFormat::Flit,
            "poetry" => PythonFormat::Poetry,
            "hatchling" => PythonFormat::Hatchling,
            _ => PythonFormat::Setuptools, // fallback
        }
    }
}

/// Rust template configuration: variant and lock file strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustConfig {
    pub variant: RustVariant,
    pub lock_strategy: RustLockStrategy,
}

/// Rust package variant (currently only one, but extensible).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RustVariant {
    /// rustPlatform.buildRustPackage (library or binary)
    Package,
}

/// Rust dependency locking strategy: cargoHash or cargoLock.lockFile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RustLockStrategy {
    /// cargoHash = "sha256-..." (for nixpkgs, remote sources)
    CargoHash,
    /// cargoLock.lockFile = ./Cargo.lock (for local development)
    LockFile {
        /// Git dependency keys needing outputHashes entries
        git_deps: Vec<String>,
    },
}

/// Go template configuration: variant and module path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoConfig {
    pub variant: GoVariant,
    /// Module path from go.mod (e.g., "github.com/user/repo")
    /// Used to suggest ldflags for version embedding.
    pub module_path: Option<String>,
}

/// Go package variant (currently only one, but extensible).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GoVariant {
    /// buildGoModule (any Go module)
    Module,
}

/// Node.js template configuration: npm or pnpm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeConfig {
    pub variant: NodeVariant,
}

/// Node.js package manager variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeVariant {
    /// buildNpmPackage (package-lock.json)
    Npm,
    /// stdenv.mkDerivation + pnpmConfigHook (pnpm-lock.yaml)
    Pnpm,
}

/// CLI-friendly template names for argument parsing.
/// These maintain backward compatibility with the original flat structure.
pub const CLI_TEMPLATES: &[&str] = &[
    "auto",
    "stdenv",
    "stdenvNoCC",
    "python_package",
    "python_application",
    "rust",
    "go",
    "npm",
    "pnpm",
    "dotnet",
    "ruby",
    "mkshell",
    "module",
    "test",
];

impl Template {
    /// Return all CLI template variant strings (for clap's possible_values).
    pub fn variants() -> Vec<&'static str> {
        CLI_TEMPLATES.to_vec()
    }

    /// Create a default Rust template (uses CargoHash strategy).
    pub fn rust() -> Self {
        Template::Rust(RustConfig {
            variant: RustVariant::Package,
            lock_strategy: RustLockStrategy::CargoHash,
        })
    }

    /// Create a default Go template.
    pub fn go() -> Self {
        Template::Go(GoConfig {
            variant: GoVariant::Module,
            module_path: None,
        })
    }

    /// Create a default Python package template (with setuptools format).
    pub fn python_package() -> Self {
        Template::Python(PythonConfig {
            variant: PythonVariant::Package,
            format: PythonFormat::Setuptools,
        })
    }

    /// Create a default Python application template (with setuptools format).
    pub fn python_application() -> Self {
        Template::Python(PythonConfig {
            variant: PythonVariant::Application,
            format: PythonFormat::Setuptools,
        })
    }

    /// Create a default npm template.
    pub fn npm() -> Self {
        Template::Node(NodeConfig {
            variant: NodeVariant::Npm,
        })
    }

    /// Create a default pnpm template.
    pub fn pnpm() -> Self {
        Template::Node(NodeConfig {
            variant: NodeVariant::Pnpm,
        })
    }

    /// Create a default stdenv template.
    pub fn stdenv() -> Self {
        Template::Stdenv(StdenvVariant::Default)
    }

    /// Create a stdenvNoCC template.
    pub fn stdenv_nocc() -> Self {
        Template::Stdenv(StdenvVariant::NoCC)
    }

    /// Parse from CLI string argument (case-insensitive).
    pub fn parse_cli(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Template::Auto),
            "stdenv" => Ok(Template::Stdenv(StdenvVariant::Default)),
            "stdenvnocc" => Ok(Template::Stdenv(StdenvVariant::NoCC)),
            "python_package" => Ok(Template::Python(PythonConfig {
                variant: PythonVariant::Package,
                format: PythonFormat::Setuptools, // default, detected later
            })),
            "python_application" => Ok(Template::Python(PythonConfig {
                variant: PythonVariant::Application,
                format: PythonFormat::Setuptools,
            })),
            "rust" => Ok(Template::Rust(RustConfig {
                variant: RustVariant::Package,
                lock_strategy: RustLockStrategy::CargoHash,
            })),
            "go" => Ok(Template::Go(GoConfig {
                variant: GoVariant::Module,
                module_path: None,
            })),
            "npm" => Ok(Template::Node(NodeConfig {
                variant: NodeVariant::Npm,
            })),
            "pnpm" => Ok(Template::Node(NodeConfig {
                variant: NodeVariant::Pnpm,
            })),
            "dotnet" => Ok(Template::Dotnet),
            "ruby" => Ok(Template::Ruby),
            "mkshell" => Ok(Template::Mkshell),
            "module" => Ok(Template::Module),
            "test" => Ok(Template::Test),
            _ => Err(format!("Unknown template: {}", s)),
        }
    }

    /// Convert to CLI string for display and serialization.
    pub fn to_cli_str(&self) -> &'static str {
        match self {
            Template::Auto => "auto",
            Template::Stdenv(StdenvVariant::Default) => "stdenv",
            Template::Stdenv(StdenvVariant::NoCC) => "stdenvNoCC",
            Template::Python(config) => match config.variant {
                PythonVariant::Package => "python_package",
                PythonVariant::Application => "python_application",
            },
            Template::Rust(_) => "rust",
            Template::Go(_) => "go",
            Template::Node(config) => match config.variant {
                NodeVariant::Npm => "npm",
                NodeVariant::Pnpm => "pnpm",
            },
            Template::Dotnet => "dotnet",
            Template::Ruby => "ruby",
            Template::Mkshell => "mkshell",
            Template::Module => "module",
            Template::Test => "test",
        }
    }

    /// Check if this is a Python template (any variant).
    pub fn is_python(&self) -> bool {
        matches!(self, Template::Python(_))
    }

    /// Get Python config if this is a Python template.
    pub fn python_config(&self) -> Option<&PythonConfig> {
        match self {
            Template::Python(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Python config.
    pub fn python_config_mut(&mut self) -> Option<&mut PythonConfig> {
        match self {
            Template::Python(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Rust template.
    pub fn is_rust(&self) -> bool {
        matches!(self, Template::Rust(_))
    }

    /// Get Rust config if this is a Rust template.
    pub fn rust_config(&self) -> Option<&RustConfig> {
        match self {
            Template::Rust(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Rust config.
    pub fn rust_config_mut(&mut self) -> Option<&mut RustConfig> {
        match self {
            Template::Rust(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Go template.
    pub fn is_go(&self) -> bool {
        matches!(self, Template::Go(_))
    }

    /// Get Go config if this is a Go template.
    pub fn go_config(&self) -> Option<&GoConfig> {
        match self {
            Template::Go(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Go config.
    pub fn go_config_mut(&mut self) -> Option<&mut GoConfig> {
        match self {
            Template::Go(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Node template (any variant).
    pub fn is_node(&self) -> bool {
        matches!(self, Template::Node(_))
    }

    /// Get Node config if this is a Node template.
    pub fn node_config(&self) -> Option<&NodeConfig> {
        match self {
            Template::Node(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Node config.
    pub fn node_config_mut(&mut self) -> Option<&mut NodeConfig> {
        match self {
            Template::Node(config) => Some(config),
            _ => None,
        }
    }
}

impl FromStr for Template {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Template::parse_cli(s)
    }
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_cli_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_templates() {
        assert_eq!("auto".parse::<Template>().unwrap(), Template::Auto);
        assert_eq!(
            "stdenv".parse::<Template>().unwrap(),
            Template::Stdenv(StdenvVariant::Default)
        );
        assert_eq!(
            "stdenvNoCC".parse::<Template>().unwrap(),
            Template::Stdenv(StdenvVariant::NoCC)
        );
        assert!("python_package".parse::<Template>().unwrap().is_python());
        assert!("python_application".parse::<Template>().unwrap().is_python());
        assert!("rust".parse::<Template>().unwrap().is_rust());
        assert!("go".parse::<Template>().unwrap().is_go());
        assert!("npm".parse::<Template>().unwrap().is_node());
        assert!("pnpm".parse::<Template>().unwrap().is_node());
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(
            "STDENV".parse::<Template>().unwrap(),
            Template::Stdenv(StdenvVariant::Default)
        );
        assert_eq!(
            "Python_Package".parse::<Template>().unwrap().to_cli_str(),
            "python_package"
        );
    }

    #[test]
    fn parse_unknown_template() {
        assert!("unknown".parse::<Template>().is_err());
    }

    #[test]
    fn to_cli_str_round_trip() {
        for variant in CLI_TEMPLATES {
            let parsed: Template = variant.parse().unwrap();
            assert_eq!(parsed.to_cli_str(), *variant);
        }
    }

    #[test]
    fn python_config_access() {
        let mut tmpl: Template = "python_package".parse().unwrap();
        assert!(tmpl.is_python());
        assert_eq!(
            tmpl.python_config().unwrap().variant,
            PythonVariant::Package
        );

        // Mutate format
        tmpl.python_config_mut().unwrap().format = PythonFormat::Flit;
        assert_eq!(tmpl.python_config().unwrap().format, PythonFormat::Flit);
    }

    #[test]
    fn rust_config_access() {
        let mut tmpl: Template = "rust".parse().unwrap();
        assert!(tmpl.is_rust());
        assert_eq!(
            tmpl.rust_config().unwrap().lock_strategy,
            RustLockStrategy::CargoHash
        );

        // Mutate lock strategy
        tmpl.rust_config_mut().unwrap().lock_strategy = RustLockStrategy::LockFile {
            git_deps: vec!["foo-0.1.0".to_string()],
        };
        assert!(matches!(
            tmpl.rust_config().unwrap().lock_strategy,
            RustLockStrategy::LockFile { .. }
        ));
    }

    #[test]
    fn node_variant_distinction() {
        let npm: Template = "npm".parse().unwrap();
        let pnpm: Template = "pnpm".parse().unwrap();

        assert!(npm.is_node());
        assert!(pnpm.is_node());

        assert!(matches!(
            npm.node_config().unwrap().variant,
            NodeVariant::Npm
        ));
        assert!(matches!(
            pnpm.node_config().unwrap().variant,
            NodeVariant::Pnpm
        ));
    }

    #[test]
    fn python_format_str_conversion() {
        assert_eq!(PythonFormat::Setuptools.as_str(), "setuptools");
        assert_eq!(PythonFormat::Flit.as_str(), "flit");
        assert_eq!(PythonFormat::from_str("poetry"), PythonFormat::Poetry);
        assert_eq!(
            PythonFormat::from_str("HATCHLING"),
            PythonFormat::Hatchling
        );
    }

    #[test]
    fn go_config_module_path() {
        let mut tmpl: Template = "go".parse().unwrap();
        assert!(tmpl.is_go());
        assert_eq!(tmpl.go_config().unwrap().module_path, None);

        tmpl.go_config_mut().unwrap().module_path = Some("github.com/user/repo".to_string());
        assert_eq!(
            tmpl.go_config().unwrap().module_path.as_deref(),
            Some("github.com/user/repo")
        );
    }
}

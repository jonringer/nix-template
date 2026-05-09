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
    /// Python UV workspace (modern Python packaging with uv2nix)
    Uv,
    /// Rust package with lock file strategy
    Rust(RustConfig),
    /// Go module with optional module path
    Go(GoConfig),
    /// Node.js package (npm or pnpm)
    Node(NodeConfig),
    /// PHP package (buildComposerProject2)
    Php(PhpConfig),
    /// Java/Maven package (buildMavenPackage)
    Maven(MavenConfig),
    /// Elixir application or library (mixRelease or buildMix)
    Elixir(ElixirConfig),
    /// Java/Gradle package (gradle.fetchDeps or manual)
    Gradle(GradleConfig),
    /// Dart application (buildDartApplication) - non-Flutter only
    Dart(DartConfig),
    /// Haskell package or application (callCabal2nix)
    Haskell(HaskellConfig),
    /// OCaml package (buildDunePackage)
    Ocaml(OcamlConfig),
    /// Scala/SBT package (sbt-derivation)
    Scala(ScalaConfig),
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
    #[allow(dead_code)]
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

/// Rust package variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RustVariant {
    /// rustPlatform.buildRustPackage (library or binary)
    Package,
    /// crane-based build with incremental caching
    Crane,
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

/// Go package variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GoVariant {
    /// buildGoModule (any Go module)
    Module,
    /// gomod2nix-based build with better dependency sharing
    Gomod2nix,
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

/// PHP template configuration: version and extensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhpConfig {
    /// PHP version to use (e.g., "81", "82", "83")
    pub version: Option<String>,
    /// PHP extensions detected from composer.json ext-* requirements
    pub extensions: Vec<String>,
}

/// Maven template configuration: JDK version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MavenConfig {
    /// JDK version to use (e.g., "17", "21")
    /// Inferred from pom.xml properties or defaulted to latest LTS
    pub jdk_version: Option<String>,
}

/// Elixir template configuration: variant and OTP version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElixirConfig {
    /// Variant: Release (mixRelease for Phoenix apps) or Library (buildMix)
    pub variant: ElixirVariant,
    /// OTP version (e.g., "27", "26", "25")
    /// Inferred from .tool-versions or defaulted to nixpkgs default
    pub otp_version: Option<String>,
}

/// Elixir package variant: Release (application) or Library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElixirVariant {
    /// mixRelease (for Phoenix apps with releases configuration)
    Release,
    /// buildMix (for hex.pm libraries)
    Library,
}

/// Gradle template configuration: variant, DSL, and JDK version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradleConfig {
    /// Variant: Gradle2nix (recommended) or Manual (less reproducible)
    pub variant: GradleVariant,
    /// DSL: Groovy (build.gradle) or Kotlin (build.gradle.kts)
    pub dsl: GradleDsl,
    /// JDK version (e.g., "17", "21")
    /// Inferred from gradle.properties or defaulted to latest LTS
    pub jdk_version: Option<String>,
}

/// Gradle build variant: Gradle2nix or Manual.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GradleVariant {
    /// gradle.fetchDeps with gradle-deps.json (requires gradle2nix tool)
    Gradle2nix,
    /// Manual stdenv.mkDerivation (less reproducible, not recommended)
    Manual,
}

/// Gradle DSL variant: Groovy or Kotlin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GradleDsl {
    /// build.gradle (Groovy DSL)
    Groovy,
    /// build.gradle.kts (Kotlin DSL)
    Kotlin,
}

/// Dart template configuration: executables and dart version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DartConfig {
    /// Executables defined in pubspec.yaml (e.g., ["myapp", "tool"])
    /// Parsed from the executables section
    pub executables: Vec<String>,
    /// Dart SDK version constraint from pubspec.yaml
    /// Not currently used but reserved for future version pinning
    pub dart_version: Option<String>,
}

/// Haskell template configuration: build system and GHC version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HaskellConfig {
    /// Build system: Cabal or Stack
    pub build_system: HaskellBuildSystem,
    /// GHC version to use (e.g., "ghc96", "ghc98")
    /// Inferred from cabal.project or stack.yaml, or defaulted to nixpkgs default
    pub ghc_version: Option<String>,
    /// Package type: executable or library (detected from .cabal file)
    pub is_executable: bool,
}

/// Haskell build system variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HaskellBuildSystem {
    /// Cabal build system (*.cabal, cabal.project)
    Cabal,
    /// Stack build system (stack.yaml)
    Stack,
}

/// OCaml template configuration: package name and OCaml version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcamlConfig {
    /// Package name from dune-project or .opam file
    pub package_name: Option<String>,
    /// OCaml version constraint (e.g., ">= 4.14")
    /// Not currently used but reserved for future version pinning
    pub ocaml_version: Option<String>,
}

/// Scala template configuration: Scala version and SBT version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalaConfig {
    /// Scala version (e.g., "2.13.12")
    /// Inferred from build.sbt if present
    pub scala_version: Option<String>,
    /// SBT version (e.g., "1.9.7")
    /// Read from project/build.properties if present
    pub sbt_version: Option<String>,
}

/// CLI-friendly template names for argument parsing.
/// These maintain backward compatibility with the original flat structure.
pub const CLI_TEMPLATES: &[&str] = &[
    "auto",
    "stdenv",
    "stdenvNoCC",
    "python_package",
    "python_application",
    "uv",
    "rust",
    "rust_crane",
    "go",
    "go_gomod2nix",
    "npm",
    "pnpm",
    "php",
    "maven",
    "elixir",
    "gradle",
    "dart",
    "haskell",
    "ocaml",
    "scala",
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

    /// Create a default UV template (modern Python with uv2nix).
    pub fn uv() -> Self {
        Template::Uv
    }

    /// Create a crane-based Rust template.
    pub fn rust_crane() -> Self {
        Template::Rust(RustConfig {
            variant: RustVariant::Crane,
            lock_strategy: RustLockStrategy::LockFile { git_deps: vec![] },
        })
    }

    /// Create a gomod2nix-based Go template.
    pub fn go_gomod2nix() -> Self {
        Template::Go(GoConfig {
            variant: GoVariant::Gomod2nix,
            module_path: None,
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

    /// Create a default PHP template.
    pub fn php() -> Self {
        Template::Php(PhpConfig {
            version: None, // Use generic 'php' attribute by default
            extensions: Vec::new(),
        })
    }

    /// Create a default Maven template.
    pub fn maven() -> Self {
        Template::Maven(MavenConfig {
            jdk_version: None, // Will be inferred from pom.xml
        })
    }

    /// Create an Elixir Release template (for Phoenix apps).
    pub fn elixir_release() -> Self {
        Template::Elixir(ElixirConfig {
            variant: ElixirVariant::Release,
            otp_version: None, // Will be inferred from .tool-versions
        })
    }

    /// Create an Elixir Library template (for hex.pm packages).
    pub fn elixir_library() -> Self {
        Template::Elixir(ElixirConfig {
            variant: ElixirVariant::Library,
            otp_version: None,
        })
    }

    /// Create a default Elixir template (defaults to Release variant).
    pub fn elixir() -> Self {
        Self::elixir_release()
    }

    /// Create a Gradle template with Gradle2nix variant.
    pub fn gradle() -> Self {
        Template::Gradle(GradleConfig {
            variant: GradleVariant::Manual, // Default to Manual; detection upgrades to Gradle2nix
            dsl: GradleDsl::Groovy,         // Will be detected later
            jdk_version: None,              // Will be inferred from gradle.properties
        })
    }

    /// Create a Dart template.
    pub fn dart() -> Self {
        Template::Dart(DartConfig {
            executables: Vec::new(), // Will be parsed from pubspec.yaml
            dart_version: None,      // Reserved for future version pinning
        })
    }

    /// Create a Haskell template (Cabal-based by default).
    pub fn haskell() -> Self {
        Template::Haskell(HaskellConfig {
            build_system: HaskellBuildSystem::Cabal,
            ghc_version: None,  // Will be inferred or use nixpkgs default
            is_executable: true, // Default assumption; will be detected from .cabal file
        })
    }

    /// Create an OCaml template.
    pub fn ocaml() -> Self {
        Template::Ocaml(OcamlConfig {
            package_name: None,   // Will be parsed from dune-project or .opam
            ocaml_version: None,  // Reserved for future version pinning
        })
    }

    /// Create a Scala template.
    pub fn scala() -> Self {
        Template::Scala(ScalaConfig {
            scala_version: None, // Will be inferred from build.sbt
            sbt_version: None,   // Will be read from project/build.properties
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
            "uv" => Ok(Template::Uv),
            "rust" => Ok(Template::Rust(RustConfig {
                variant: RustVariant::Package,
                lock_strategy: RustLockStrategy::CargoHash,
            })),
            "rust_crane" => Ok(Template::Rust(RustConfig {
                variant: RustVariant::Crane,
                lock_strategy: RustLockStrategy::LockFile { git_deps: vec![] },
            })),
            "go" => Ok(Template::Go(GoConfig {
                variant: GoVariant::Module,
                module_path: None,
            })),
            "go_gomod2nix" => Ok(Template::Go(GoConfig {
                variant: GoVariant::Gomod2nix,
                module_path: None,
            })),
            "npm" => Ok(Template::Node(NodeConfig {
                variant: NodeVariant::Npm,
            })),
            "pnpm" => Ok(Template::Node(NodeConfig {
                variant: NodeVariant::Pnpm,
            })),
            "php" => Ok(Template::Php(PhpConfig {
                version: None, // Use generic 'php' attribute by default
                extensions: Vec::new(),
            })),
            "maven" => Ok(Template::Maven(MavenConfig {
                jdk_version: None, // Will be inferred from pom.xml
            })),
            "elixir" => Ok(Template::Elixir(ElixirConfig {
                variant: ElixirVariant::Release, // Default to Release, can be detected later
                otp_version: None,
            })),
            "gradle" => Ok(Template::Gradle(GradleConfig {
                variant: GradleVariant::Manual, // Default to Manual; detection upgrades to Gradle2nix
                dsl: GradleDsl::Groovy,         // Will be detected later
                jdk_version: None,
            })),
            "dart" => Ok(Template::Dart(DartConfig {
                executables: Vec::new(),
                dart_version: None,
            })),
            "haskell" => Ok(Template::Haskell(HaskellConfig {
                build_system: HaskellBuildSystem::Cabal,
                ghc_version: None,
                is_executable: true,
            })),
            "ocaml" => Ok(Template::Ocaml(OcamlConfig {
                package_name: None,
                ocaml_version: None,
            })),
            "scala" => Ok(Template::Scala(ScalaConfig {
                scala_version: None,
                sbt_version: None,
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
            Template::Uv => "uv",
            Template::Rust(config) => match config.variant {
                RustVariant::Package => "rust",
                RustVariant::Crane => "rust_crane",
            },
            Template::Go(config) => match config.variant {
                GoVariant::Module => "go",
                GoVariant::Gomod2nix => "go_gomod2nix",
            },
            Template::Node(config) => match config.variant {
                NodeVariant::Npm => "npm",
                NodeVariant::Pnpm => "pnpm",
            },
            Template::Php(_) => "php",
            Template::Maven(_) => "maven",
            Template::Elixir(_) => "elixir",
            Template::Gradle(_) => "gradle",
            Template::Dart(_) => "dart",
            Template::Haskell(_) => "haskell",
            Template::Ocaml(_) => "ocaml",
            Template::Scala(_) => "scala",
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn rust_config(&self) -> Option<&RustConfig> {
        match self {
            Template::Rust(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Rust config.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn go_config(&self) -> Option<&GoConfig> {
        match self {
            Template::Go(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Go config.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn node_config(&self) -> Option<&NodeConfig> {
        match self {
            Template::Node(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Node config.
    #[allow(dead_code)]
    pub fn node_config_mut(&mut self) -> Option<&mut NodeConfig> {
        match self {
            Template::Node(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a PHP template.
    pub fn is_php(&self) -> bool {
        matches!(self, Template::Php(_))
    }

    /// Get PHP config if this is a PHP template.
    #[allow(dead_code)]
    pub fn php_config(&self) -> Option<&PhpConfig> {
        match self {
            Template::Php(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable PHP config.
    #[allow(dead_code)]
    pub fn php_config_mut(&mut self) -> Option<&mut PhpConfig> {
        match self {
            Template::Php(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Maven template.
    pub fn is_maven(&self) -> bool {
        matches!(self, Template::Maven(_))
    }

    /// Get Maven config if this is a Maven template.
    #[allow(dead_code)]
    pub fn maven_config(&self) -> Option<&MavenConfig> {
        match self {
            Template::Maven(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Maven config.
    #[allow(dead_code)]
    pub fn maven_config_mut(&mut self) -> Option<&mut MavenConfig> {
        match self {
            Template::Maven(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is an Elixir template.
    pub fn is_elixir(&self) -> bool {
        matches!(self, Template::Elixir(_))
    }

    /// Get Elixir config if this is an Elixir template.
    #[allow(dead_code)]
    pub fn elixir_config(&self) -> Option<&ElixirConfig> {
        match self {
            Template::Elixir(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Elixir config.
    #[allow(dead_code)]
    pub fn elixir_config_mut(&mut self) -> Option<&mut ElixirConfig> {
        match self {
            Template::Elixir(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Gradle template.
    pub fn is_gradle(&self) -> bool {
        matches!(self, Template::Gradle(_))
    }

    /// Get Gradle config if this is a Gradle template.
    #[allow(dead_code)]
    pub fn gradle_config(&self) -> Option<&GradleConfig> {
        match self {
            Template::Gradle(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Gradle config.
    #[allow(dead_code)]
    pub fn gradle_config_mut(&mut self) -> Option<&mut GradleConfig> {
        match self {
            Template::Gradle(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Dart template.
    pub fn is_dart(&self) -> bool {
        matches!(self, Template::Dart(_))
    }

    /// Get Dart config if this is a Dart template.
    #[allow(dead_code)]
    pub fn dart_config(&self) -> Option<&DartConfig> {
        match self {
            Template::Dart(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Dart config.
    #[allow(dead_code)]
    pub fn dart_config_mut(&mut self) -> Option<&mut DartConfig> {
        match self {
            Template::Dart(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Haskell template.
    pub fn is_haskell(&self) -> bool {
        matches!(self, Template::Haskell(_))
    }

    /// Get Haskell config if this is a Haskell template.
    #[allow(dead_code)]
    pub fn haskell_config(&self) -> Option<&HaskellConfig> {
        match self {
            Template::Haskell(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Haskell config.
    #[allow(dead_code)]
    pub fn haskell_config_mut(&mut self) -> Option<&mut HaskellConfig> {
        match self {
            Template::Haskell(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is an OCaml template.
    pub fn is_ocaml(&self) -> bool {
        matches!(self, Template::Ocaml(_))
    }

    /// Get OCaml config if this is an OCaml template.
    #[allow(dead_code)]
    pub fn ocaml_config(&self) -> Option<&OcamlConfig> {
        match self {
            Template::Ocaml(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable OCaml config.
    #[allow(dead_code)]
    pub fn ocaml_config_mut(&mut self) -> Option<&mut OcamlConfig> {
        match self {
            Template::Ocaml(config) => Some(config),
            _ => None,
        }
    }

    /// Check if this is a Scala template.
    pub fn is_scala(&self) -> bool {
        matches!(self, Template::Scala(_))
    }

    /// Get Scala config if this is a Scala template.
    #[allow(dead_code)]
    pub fn scala_config(&self) -> Option<&ScalaConfig> {
        match self {
            Template::Scala(config) => Some(config),
            _ => None,
        }
    }

    /// Get mutable Scala config.
    #[allow(dead_code)]
    pub fn scala_config_mut(&mut self) -> Option<&mut ScalaConfig> {
        match self {
            Template::Scala(config) => Some(config),
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
        assert!("python_application"
            .parse::<Template>()
            .unwrap()
            .is_python());
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
        assert_eq!(PythonFormat::from_str("HATCHLING"), PythonFormat::Hatchling);
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

    #[test]
    fn php_template_parsing() {
        let tmpl: Template = "php".parse().unwrap();
        assert!(tmpl.is_php());
        assert_eq!(tmpl.to_cli_str(), "php");
        assert_eq!(tmpl.php_config().unwrap().version, None); // Uses generic 'php' by default
        assert!(tmpl.php_config().unwrap().extensions.is_empty());
    }

    #[test]
    fn php_config_access() {
        let mut tmpl: Template = "php".parse().unwrap();
        assert!(tmpl.is_php());
        assert_eq!(tmpl.php_config().unwrap().version, None); // Default is None (uses generic 'php')

        // Mutate version and extensions
        tmpl.php_config_mut().unwrap().version = Some("82".to_string());
        tmpl.php_config_mut().unwrap().extensions = vec!["pdo".to_string(), "gd".to_string()];

        assert_eq!(tmpl.php_config().unwrap().version, Some("82".to_string()));
        assert_eq!(
            tmpl.php_config().unwrap().extensions,
            vec!["pdo".to_string(), "gd".to_string()]
        );
    }

    #[test]
    fn maven_template_parsing() {
        let tmpl: Template = "maven".parse().unwrap();
        assert!(tmpl.is_maven());
        assert_eq!(tmpl.to_cli_str(), "maven");
        assert_eq!(tmpl.maven_config().unwrap().jdk_version, None); // Default inferred from pom.xml
    }

    #[test]
    fn maven_config_access() {
        let mut tmpl: Template = "maven".parse().unwrap();
        assert!(tmpl.is_maven());
        assert_eq!(tmpl.maven_config().unwrap().jdk_version, None);

        // Mutate jdk_version
        tmpl.maven_config_mut().unwrap().jdk_version = Some("21".to_string());

        assert_eq!(
            tmpl.maven_config().unwrap().jdk_version,
            Some("21".to_string())
        );
    }

    #[test]
    fn elixir_template_parsing() {
        let tmpl: Template = "elixir".parse().unwrap();
        assert!(tmpl.is_elixir());
        assert_eq!(tmpl.to_cli_str(), "elixir");
        assert_eq!(
            tmpl.elixir_config().unwrap().variant,
            ElixirVariant::Release
        ); // Default
        assert_eq!(tmpl.elixir_config().unwrap().otp_version, None);
    }

    #[test]
    fn elixir_config_access() {
        let mut tmpl: Template = "elixir".parse().unwrap();
        assert!(tmpl.is_elixir());
        assert_eq!(
            tmpl.elixir_config().unwrap().variant,
            ElixirVariant::Release
        );

        // Mutate variant and otp_version
        tmpl.elixir_config_mut().unwrap().variant = ElixirVariant::Library;
        tmpl.elixir_config_mut().unwrap().otp_version = Some("27".to_string());

        assert_eq!(
            tmpl.elixir_config().unwrap().variant,
            ElixirVariant::Library
        );
        assert_eq!(
            tmpl.elixir_config().unwrap().otp_version,
            Some("27".to_string())
        );
    }

    #[test]
    fn elixir_variant_helpers() {
        let release = Template::elixir_release();
        assert!(matches!(
            release.elixir_config().unwrap().variant,
            ElixirVariant::Release
        ));

        let library = Template::elixir_library();
        assert!(matches!(
            library.elixir_config().unwrap().variant,
            ElixirVariant::Library
        ));
    }

    #[test]
    fn gradle_template_parsing() {
        let tmpl: Template = "gradle".parse().unwrap();
        assert!(tmpl.is_gradle());
        assert_eq!(tmpl.to_cli_str(), "gradle");
        assert_eq!(tmpl.gradle_config().unwrap().variant, GradleVariant::Manual); // Default (detection upgrades to Gradle2nix when gradle-deps.json exists)
        assert_eq!(tmpl.gradle_config().unwrap().dsl, GradleDsl::Groovy); // Default
        assert_eq!(tmpl.gradle_config().unwrap().jdk_version, None);
    }

    #[test]
    fn gradle_config_access() {
        let mut tmpl: Template = "gradle".parse().unwrap();
        assert!(tmpl.is_gradle());
        assert_eq!(tmpl.gradle_config().unwrap().variant, GradleVariant::Manual);

        // Mutate variant, dsl, and jdk_version
        tmpl.gradle_config_mut().unwrap().variant = GradleVariant::Gradle2nix;
        tmpl.gradle_config_mut().unwrap().dsl = GradleDsl::Kotlin;
        tmpl.gradle_config_mut().unwrap().jdk_version = Some("21".to_string());

        assert_eq!(
            tmpl.gradle_config().unwrap().variant,
            GradleVariant::Gradle2nix
        );
        assert_eq!(tmpl.gradle_config().unwrap().dsl, GradleDsl::Kotlin);
        assert_eq!(
            tmpl.gradle_config().unwrap().jdk_version,
            Some("21".to_string())
        );
    }
}

# Changelog

## v1.0.0 (Unreleased)

- Breaking Changes:
  - Removed Qt template (deprecated, use stdenv.mkDerivation with wrapQtAppsHook instead)
  - Removed legacy `python` template (use `python_package` or `python_application` instead)
  - Removed `--nixpkgs` flag (obsolete), replaced by `--by-name`
  - All templates now use `finalAttrs` pattern instead of `rec` for better override composition

- Additions:
  - Templates:
    - Added `python_package` template (for Python libraries using buildPythonPackage)
    - Added `python_application` template (for Python applications using buildPythonApplication)
    - Added `stdenvNoCC` template (for packages that don't need a compiler)
    - Added `npm` template for Node.js packages using buildNpmPackage
    - Added `pnpm` template for pnpm-based packages using fetchPnpmDeps with stdenv.mkDerivation
    - Added `dotnet` template for .NET packages using buildDotnetModule
    - Added `ruby` template for Ruby applications using bundlerApp
    - Added `php` template for Composer-based PHP projects using buildComposerProject2
    - Added `maven` template for Maven-based Java projects using buildMavenPackage
    - Added `elixir` template for Mix-based Elixir applications using mixRelease/buildMix
    - Added `gradle` template for Gradle-based Java/Kotlin projects using gradle.fetchDeps
    - Added `dart` template for Dart applications using buildDartApplication
    - Added `haskell` template for Haskell packages using haskellPackages.callCabal2nix
    - Added `ocaml` template for OCaml packages using buildDunePackage
    - Added `scala` template for Scala/SBT packages using stdenv.mkDerivation with sbt-derivation
    - Added `clojure` template for Clojure projects using deps.edn or Leiningen with clj-nix
    - Added `perl` template for Perl modules using buildPerlPackage or buildPerlModule
    - Added `lua` template for Lua packages and applications using buildLuaPackage or buildLuaApplication
    - Added `r` template for R packages using rPackages.buildRPackage
    - Added `auto` template type for automatic project type detection
  - CLI Flags:
    - Added `--by-name` flag for RFC 140 support (pkgs/by-name directory structure)
    - Added `--binputs` and `--nbinputs` flags to manually specify buildInputs and nativeBuildInputs
    - Added `--init-npins` flag to initialize npins-based (v0.4.0) projects (alternative to flakes)
    - Added `--skip-vendor-hash` flag to skip automatic vendor hash prefetching
    - Added `--skip-infer-deps` flag to skip automatic dependency inference
  - Fetcher Support:
    - Added GitLab fetcher support with `--from-url`
    - Added Gitea fetcher support with `--from-url`
  - Dependency Inference:
    - Rust: Infers dependencies from Cargo.toml and scans Cargo.lock for crates with native dependencies
    - Go: Infers build inputs from CGO directives in Go source files
    - Ruby: Maps common gems (nokogiri, pg, mysql2, etc.) from Gemfile.lock to nixpkgs dependencies
    - PHP: Detects PHP extensions from composer.json ext-* requirements and maps common packages to native dependencies
    - Maven: Infers JDK version from pom.xml properties and maps JDBC drivers to native dependencies
    - Elixir: Detects variant (Release/Library) from mix.exs and maps Mix packages with NIFs to native dependencies
    - Gradle: Infers JDK version from gradle.properties and build.gradle, detects Gradle DSL variant (Groovy/Kotlin)
    - Dart: Parses executables from pubspec.yaml and excludes Flutter projects
    - Haskell: Detects build system (Cabal/Stack) and parses .cabal files to distinguish executables from libraries
    - OCaml: Extracts package name from dune-project or .opam files
    - Scala: Extracts Scala version from build.sbt and SBT version from project/build.properties
    - Clojure: Detects build tool (Deps/Leiningen) from deps.edn or project.clj and infers JDK version from build files
    - Perl: Detects build system (MakeMaker/Module::Build) from Makefile.PL or Build.PL and parses META.json/META.yml for dependencies
    - Lua: Detects variant (Package/Application) and Lua version (5.1-5.4/LuaJIT) from .rockspec files
    - R: Parses DESCRIPTION file for package metadata, R version requirements, and dependencies (Depends, Imports, LinkingTo)
    - CMake: Parses find_package() and find_dependency() calls for common dependencies (OpenSSL, ZLIB, Qt, Boost, etc.)
    - Meson: Parses dependency() calls for common dependencies (zlib, openssl, gtk, glib, etc.)
    - Autotools: Detects PKG_CHECK_MODULES in configure.ac and adds pkg-config to nativeBuildInputs
  - Build System Detection:
    - Auto-detects CMakeLists.txt and adds cmake to nativeBuildInputs
    - Auto-detects meson.build and adds meson + ninja to nativeBuildInputs
  - Auto-detection:
    - Automatic project type detection from source code with `auto` template
    - Recognizes Rust projects (via Cargo.toml)
    - Recognizes Go projects (via go.mod)
    - Recognizes npm projects (via package-lock.json or package.json)
    - Recognizes pnpm projects (via pnpm-lock.yaml)
    - Recognizes .NET projects (via *.csproj, *.fsproj, or *.sln files)
    - Recognizes Ruby projects (via Gemfile.lock or Gemfile)
    - Recognizes PHP projects (via composer.lock or composer.json)
    - Recognizes Maven projects (via pom.xml)
    - Recognizes Elixir projects (via mix.lock or mix.exs)
    - Recognizes Gradle projects (via build.gradle, build.gradle.kts, or gradle-deps.json)
    - Recognizes Dart projects (via pubspec.lock or pubspec.yaml, excludes Flutter projects)
    - Recognizes Haskell projects (via *.cabal, cabal.project, or stack.yaml)
    - Recognizes OCaml projects (via dune-project or *.opam files)
    - Recognizes Scala projects (via build.sbt)
    - Recognizes R projects (via DESCRIPTION or *.Rproj files)
    - Project file inference for dotnet template when using --from-url (automatically detects .csproj, .fsproj, or .sln)
  - Project Structure:
    - Normalized `nix/` directory structure for both flake and npins-based projects
    - Organized layout for packages, overlays, and modules
  - Dependency Hash Prefetching:
    - Automatically prefetches vendor hashes for Rust (Cargo.lock), Go (go.sum), npm (package-lock.json), and pnpm (pnpm-lock.yaml)
    - Can be disabled with `--skip-vendor-hash` flag
  - UI Improvements:
    - Fuzzy search and tab completion for interactive prompts
  - Pattern Improvements:
    - Modern `finalAttrs` pattern is now default for all package templates (stdenv, Python, Ruby, Rust, Go, npm, pnpm, dotnet)

- Fixes:
  - PyPI fetcher now gracefully handles missing metadata

## v0.4.1

- Additions:
  - Python template defaults to adding a 'format = "setuptools";' to align with nixpkgs preferences

- Fixes:
  - Clearer error when repository doesn't exist
  - Minor pypi serialization fixes

## v0.4.0

- Breaking Changes:
  - `overlay` for flake template has been moved to `overlays.default` to align with upstream changes
  - `-u` will now use an sri hash, to align with nix 2.4+ behavior

- Additions:
  - `-u` when fetching from pypi will now automatically add dependencies

- Fixes:
  - Fix failure with pypi responses not containing a platform
  - `-u` with pypi will now filter out pre-releases when determining latest release
  - Default to repo name when using `-u`

## v0.3.0

- Breaking Changes:
  - `overlays` exposed in flake are now an attr set, to better align with more recent nix versions

- Additions:
  - `aarch64-darwin` added to flake system defaults

- Improvements:
  - Serialization errors will now mention which assumption caused the failure @blaggacao
  - Updated github.com auto-detected licenses to include recently added `Apache License 2.0`
  - Fixed usage of `mkApp` inside flake tempalte

- Fixes:
  - Fix unprefixed versions being generated as `version = "version";` @blaggacao
  - Fixed directories being passed as `[PATH]` not becoming `dir/default.nix`

## v0.2.0

- Breaking Changes / Behaviors:
  - Flake template now requires -p,--pname
  - Nix expresions now have input attrs in comma-leading style (one input per line)

- Fixes:
  - --from-url no longer errors with --nixpkgs when a pname is not supplied
  - Fix differences of writing to stdout vs file
  - Update flake template (overlay usage)
  - Failures from already existing file locations occur sooner
    - Particularly irritating with `--from-url`, which would compute release and sha256 info

## v0.1.4

- Cleanup pypi noise

## v0.1.3

- Fix Cargo.lock file

## v0.1.2

- Add `-u, --from-url` support to pypi.org
- Fix crash when github's hompage url is null when used with `-u`
- Add mention of `GITHUB_TOKEN` to usage

## v0.1.1

- Add nixos module template
- Add nixos test template
- Add flake template
- Add `-u,--from-url` option
  - Github supported

## v0.1.0

- Add the following templates:
  - stdenv
  - python
  - mkshell
  - go
  - rust
  - qt
- Allow users to configure persistent maintainer name and nixpkgs location
- Add shell completions

# Changelog

## v0.4.2 (Unreleased)

- Breaking Changes:
  - Removed Qt template (deprecated, use stdenv.mkDerivation with wrapQtAppsHook instead)
  - Removed legacy `python` template (use `python_package` or `python_application` instead)
  - All templates now use `finalAttrs` pattern instead of `rec` for better override composition

- Additions:
  - Modern `finalAttrs` pattern is now default for all package templates (stdenv, Python, Rust, Go, npm, pnpm, dotnet)
  - Self-references now use `finalAttrs.pname` and `finalAttrs.version`
  - PyPI fetcher uses `inherit (finalAttrs) pname version;` syntax
  - Added `npm` template for Node.js packages using buildNpmPackage
  - Added `pnpm` template for pnpm-based packages using fetchPnpmDeps with stdenv.mkDerivation
  - Added `dotnet` template for .NET packages using buildDotnetModule
  - Added `ruby` template for Ruby applications using bundlerApp
  - Dependency hash prefetching now supports npm and pnpm templates (requires package-lock.json/pnpm-lock.yaml in repository)
  - Dependency inference for ruby template from Gemfile.lock (maps common gems like nokogiri, pg, mysql2 to their nixpkgs dependencies)
  - Auto-detection now recognizes npm and pnpm projects (via pnpm-lock.yaml, package-lock.json, or package.json)
  - Auto-detection now recognizes .NET projects (via *.csproj, *.fsproj, or *.sln files)
  - Auto-detection now recognizes Ruby projects (via Gemfile.lock or Gemfile)
  - Project file inference for dotnet template when using --from-url (automatically detects .csproj, .fsproj, or .sln)

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

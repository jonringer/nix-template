# Changelog

## Unreleased

- Breaking Changes:
  - `overlay` for flake template has been moved to `overlays.default` to align with upstream changes

- Fixes:
  - Fix failure with pypi responses not containing a platform

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

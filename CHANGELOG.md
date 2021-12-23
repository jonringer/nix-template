# Changelog

## Unreleased

- Fix differences of writing to stdout vs file
- Update flake template (overlay usage)
- Flake template now requires -p,--pname
- --from-url no longer errors with --nixpkgs when a pname is not supplied
- Nix expresions now have input attrs in comma-leading style (one input per line)

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

# Nix-template

*NOTE:* This is still WIP, but should be useful in most situations

Make creating nix expressions easy. Provide a nice way to create largely boilerplate nix-expressions.

[![Packaging status](https://repology.org/badge/vertical-allrepos/nix-template.svg)](https://repology.org/project/nix-template/versions)

## Roadmap

- [ ] Finalize cli semantics
- Ease usage with nixpkgs repo
  - [X] Write to correct location using path
    - [X] Improve logic around directories vs files
    - [X] Improve template-specific items
      - [X] generate buildGoModule's vendorHash
      - [X] generate buildRustPackages's cargoHash
      - [X] generate npm/pnpm dependency hashes
  - [X] Print top-level addition statement
  - [X] Support RFC 140 with `--by-name` flag
- Support Language/frameworks/usage templates:
  - [X] Stdenv / stdenvNoCC
  - [X] Python (python_package, python_application)
  - [X] mkShell
  - [X] Go
  - [X] Rust
  - [X] npm
  - [X] pnpm
  - [X] .NET
  - [X] Ruby
  - [X] Flakes
  - [X] NixOS Module
  - [X] NixOS Test
  - [X] npins
  - [ ] Haskell
  - [ ] and many more...
- [ ] Add option (-d, --documentation-url) to embed noob-friendly comments and explanations about common usage patterns
- Allow contributor information to be set locally (similar to git settings)
  - [X] Set maintainer name through `$XDG_CONFIG_HOME`
  - [X] Set nixpkgs-root path through `$XDG_CONFIG_HOME`
- Better integration with fetchers
  - Automatically determine version and sha256
    - [X] Github (need a way to pass owner and repo)
    - [X] Pypi (will need a way to pass pypi pname, as it may differ from installable path)
    - [X] GitLab
    - [X] Gitea
- [X] Implement shell completion (nix-template completions <SHELL>)
- [X] Implement automatic project type detection with `auto` template
- [X] Implement automatic dependency inference for Rust, Go, Ruby, CMake, and Meson

## Current Usage (--from-url, supports GitHub, GitLab, Gitea, and PyPI)

```bash
$ nix-template rust -n --from-url github.com/jonringer/nix-template
Creating directory: /home/jon/projects/nixpkgs/pkgs/applications/misc/nix-template
Generating rust expression at /home/jon/projects/nixpkgs/pkgs/applications/misc/nix-template/default.nix
Please add the following line to the approriate file in top-level:

  nix-template = callPackage ../applications/misc/nix-template { };
```
The resulting file:
```
# $NIXPKGS_ROOT/pkgs/applications/misc/nix-template/default.nix
{ lib
, rustPlatform
, fetchFromGitHub
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "nix-template";
  version = "0.3.0";

  src = fetchFromGitHub {
    owner = "jonringer";
    repo = finalAttrs.pname;
    rev = "v${finalAttrs.version}";
    sha256 = "sha256-5redgssfwbNEgpjmakIcU8cL4Xg1kPvyK88v+xMqAtw=";
  };

  cargoSha256 = "0000000000000000000000000000000000000000000000000000";

  buildInputs = [ ];

  meta = with lib; {
    description = "Make creating nix expressions easy";
    homepage = "https://github.com/jonringer/nix-template";
    license = licenses.cc0;
    maintainers = with maintainers; [ jonringer ];
  };
})
```

## Current Usage (Generically)

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package (using RFC 140 by-name structure)
$ nix-template python_package --by-name --pname requests -f pypi -l asl20
Creating directory: /home/jon/projects/nixpkgs/pkgs/by-name/re/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/by-name/re/requests/package.nix
```
```nix
# pkgs/by-name/re/requests/package.nix
{ lib
, buildPythonPackage
, fetchPypi
}:

buildPythonPackage (finalAttrs: {
  pname = "requests";
  version = "0.0.1";

  src = fetchPypi {
    inherit (finalAttrs) pname version;
    sha256 = "0000000000000000000000000000000000000000000000000000";
  };

  propagatedBuildInputs = [ ];

  pythonImportsCheck = [ "requests" ];

  meta = with lib; {
    description = "CHANGEME";
    homepage = "https://github.com/CHANGEME/requests/";
    license = licenses.asl20;
    maintainer = with maintainers; [ jonringer ];
  };
})
```

## Key Features

### Automatic Project Detection
Use the `auto` template to automatically detect project type from source code:
```bash
$ nix-template auto --from-url github.com/user/project
# Automatically detects if it's Rust, Go, Python, npm, pnpm, .NET, or Ruby
```

### Dependency Inference
Automatically infers dependencies for supported languages:
- **Rust**: Scans `Cargo.toml` and `Cargo.lock` for native dependencies
- **Go**: Detects CGO directives and maps to nixpkgs inputs
- **Ruby**: Maps gems from `Gemfile.lock` to nixpkgs dependencies
- **CMake/Meson**: Parses build files for common dependencies
- **Python**: Fetches dependencies from PyPI metadata

Use `--skip-infer-deps` to disable this feature.

### Vendor Hash Prefetching
Automatically prefetches and calculates vendor hashes for:
- Rust (`Cargo.lock`)
- Go (`go.sum`)
- npm (`package-lock.json`)
- pnpm (`pnpm-lock.yaml`)

Use `--skip-vendor-hash` to disable this feature.

### Multiple Fetcher Support
Supports fetching from:
- GitHub
- GitLab
- Gitea
- PyPI

### RFC 140 Support
Use `--by-name` flag to generate packages using the modern `pkgs/by-name` directory structure.

### Project Templates
Initialize new projects with flake or npins-based setups:
```bash
$ nix-template flake --pname my-project /path/to/project
$ nix-template npins --pname my-project /path/to/project
```

### Installation

from nixpkgs (unstable, not available in 20.03):
```
$ nixenv -iA nix-template
```

with nix-cli (from this repository):
```
$ nix-env -f default.nix -iA ""
```

with cargo
```
$ cargo install --path .
```

### Development

Installing depedencies on nixpkgs:
```
nix-shell
# or
nix develop
```

Other platforms, you'll need the following dependencies:
  - cargo
  - rustc
  - rust-clippy

## Advanced Usage with Automatic Dependency Inference

The example below shows automatic dependency detection from PyPI, which fetches all required Python dependencies:

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package with automatic dependency detection and hash prefetching
$ nix-template python_package -u https://pypi.org/project/requests/
Determining latest release for requests
Fetching PyPI metadata and dependencies...
Creating directory: /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/default.nix

For RFC 140 by-name structure, use --by-name flag instead.
```

The generated file includes automatically detected dependencies:

```nix
{ lib
, buildPythonPackage
, fetchPypi
, certifi
, charset-normalizer
, idna
, urllib3
}:

buildPythonPackage (finalAttrs: {
  pname = "requests";
  version = "2.28.1";

  src = fetchPypi {
    inherit (finalAttrs) pname version;
    sha256 = "sha256-fFWZsQL+3apmHIJsVqtP7ii/0X9avKHrvj5/GdfJeYM=";
  };

  propagatedBuildInputs = [
    certifi
    charset-normalizer
    idna
    urllib3
  ];

  pythonImportsCheck = [ "requests" ];

  meta = with lib; {
    description = "Python HTTP for Humans";
    homepage = "https://requests.readthedocs.io";
    license = licenses.asl20;
    maintainers = with maintainers; [ jonringer ];
  };
})
```


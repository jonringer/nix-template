# Nix-template

Make creating nix expressions easy. Provide a nice way to create largely boilerplate nix-expressions.

[![Packaging status](https://repology.org/badge/vertical-allrepos/nix-template.svg)](https://repology.org/project/nix-template/versions)

## Current Usage (--from-url, supports GitHub, GitLab, Gitea, and PyPI)

```bash
$ /home/jon/projects/nix-template/target/release/nix-template rust --from-url github.com/jonringer/nix-template ./package.nix
Determining latest release for nix-template
Determining sha256 for nix-template
Prefetching cargoHash for nix-template (this may take a while)...
Determined cargoHash = sha256-cLSGWOyBQLv235TeYqSVg/f0Zmcnpj+RshINN69JYEU=
Materialising source to inspect Cargo.toml/Cargo.lock...
Inferred 1 buildInputs (["openssl"]) and 1 nativeBuildInputs (["pkg-config"])
Generated a rust nix expression at /home/jon/projects/nix-template/package.nix
```
The resulting file:
```
$ cat ./package.nix
{ lib
, rustPlatform
, fetchFromGitHub
, pkg-config
, openssl
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "nix-template";
  version = "0.4.1";

  src = fetchFromGitHub {
    owner = "jonringer";
    repo = finalAttrs.pname;
    rev = "v${finalAttrs.version}";
    sha256 = "sha256-42u5FmTIKHpfQ2zZQXIrFkAN2/XvU0wWnCRrQkQzcNI=";
  };

  cargoHash = "sha256-cLSGWOyBQLv235TeYqSVg/f0Zmcnpj+RshINN69JYEU=";

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    openssl
  ];

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
# For use with --by-name
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package (using RFC 140 by-name structure), inferring template and dependencies
NIXPKGS_ROOT=/home/jon/projects/nixpkgs /home/jon/projects/nix-template/target/release/nix-template auto --by-name --from-url github.com/jonringer/nix-template
Determining latest release for nix-template
Determining sha256 for nix-template
Materialising source to detect project type...
nix-template: auto-detected template 'rust' (found Cargo.toml)
Prefetching cargoHash for nix-template (this may take a while)...
Determined cargoHash = sha256-cLSGWOyBQLv235TeYqSVg/f0Zmcnpj+RshINN69JYEU=
Materialising source to inspect Cargo.toml/Cargo.lock...
Inferred 1 buildInputs (["openssl"]) and 1 nativeBuildInputs (["pkg-config"])
Generated a rust nix expression at /home/jon/projects/nixpkgs/pkgs/by-name/ni/nix-template/package.nix
```

## Key Features

### Automatic Project Detection
Use the `auto` template to automatically detect project type from source code:
```bash
$ nix-template auto --from-url github.com/user/project
# Automatically detects if it's Rust, Go, Python, UV, npm, pnpm, PHP, .NET, or Ruby
```

### Available Template Variants

**Standard Templates:**
- `stdenv` / `stdenvNoCC` - Generic stdenv-based builds
- `python_package` / `python_application` - Python packages (buildPythonPackage/buildPythonApplication)
- `rust` - Rust packages (rustPlatform.buildRustPackage)
- `go` - Go modules (buildGoModule)
- `npm` / `pnpm` - Node.js packages
- `php` - PHP packages with Composer (php.buildComposerProject2)
  - Uses generic `php` attribute (auto-tracks nixpkgs default version)
  - Automatically detects PHP extensions from `composer.json`
  - Generates `php.buildEnv` wrapper when extensions are required
  - Detects version requirements (e.g., `"php": "^8.2"`) to use specific versions when needed
- `dotnet` - .NET packages
- `ruby` - Ruby gems
- `mkshell` - Development shells
- `module` - NixOS modules
- `test` - NixOS integration tests

**Modern Packaging Variants:**
- `uv` - Python projects using UV package manager (detected via `uv.lock`)
  - **Note**: UV projects work best with flake-based workflows. Initialize with:
    ```bash
    nix flake init -t github:pyproject-nix/uv2nix#hello-world
    ```
- `rust_crane` - Rust builds with incremental caching via crane
  - Better incremental builds and caching than buildRustPackage
  - See: https://crane.dev/
- `go_gomod2nix` - Go builds with better dependency sharing
  - Uses gomod2nix.toml for dependency management
  - See: https://github.com/nix-community/gomod2nix

### Dependency Inference
Automatically infers dependencies for supported languages:
- **Rust**: Scans `Cargo.toml` and `Cargo.lock` for native dependencies
- **Go**: Detects CGO directives and maps to nixpkgs inputs
- **PHP**: Detects extensions (`ext-*`) and native libraries from `composer.json`
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
Initialize new projects with flake or npins-based setups (will prompt you for additional information):
```bash
$ nix-template --init-flake --pname my-project
$ nix-template --init-npins --pname my-project
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

using flakes
```
$ nix run github:jonringer/nix-template
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

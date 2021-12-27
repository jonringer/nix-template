# Nix-template

*NOTE:* This is still WIP, but should be useful in most situations

Make creating nix expressions easy. Provide a nice way to create largely boilerplate nix-expressions.

[![Packaging status](https://repology.org/badge/vertical-allrepos/nix-template.svg)](https://repology.org/project/nix-template/versions)

## Roadmap

- [ ] Finalize cli semantics
- Ease usage with nixpkgs repo
  - [X] Write to correct location using path
    - [X] Improve logic around directories vs files
    - [ ] Improve template-specific items
      - [ ] generate buildGoModule's depsSha256
      - [ ] generate buildRustPackages's cargoSha256
  - [X] Print top-level addition statement
- Support Language/frameworks/usage templates:
  - [X] Stdenv
  - [X] Python
  - [X] mkShell
  - [x] Qt
  - [x] Go
  - [x] Rust
  - [x] Flakes
  - [x] NixOS Module
  - [x] NixOS Test
  - [ ] Haskell
  - [ ] and many more...
- [ ] Add option (--comments?) to embed noob-friendly comments and explanations about common usage patterns
- Allow contributor information to be set locally (similar to git settings)
  - [X] Set maintainer name through `$XDG_CONFIG_HOME`
  - [X] Set nixpkgs-root path through `$XDG_CONFIG_HOME`
- Better integration with fetchers
  - Automatically determine version and sha256
    - [X] Github (need a way to pass owner and repo)
    - [X] Pypi (will need a way to pass pypi pname, as it may differ from installable path)
- [X] Implement shell completion (nix-template completions <SHELL>)

## Current Usage (github and pypi only)

```bash
$ nix-template rust -n --from-url github.com/jonringer/nix-template
Creating directory: /home/jon/projects/nixpkgs/pkgs/applications/misc/nix-template
Generating python expression at /home/jon/projects/nixpkgs/pkgs/applications/misc/nix-template/default.nix
Please add the following line to the approriate file in top-level:

  nix-template = callPackage ../applications/misc/nix-template { };
```
The resulting file:
```
# $NIXPKGS_ROOT/pkgs/applications/misc/nix-template/default.nix
{ lib, rustPlatform, fetchFromGitHub }:

rustPlatform.buildRustPackage rec {
  pname = "nix-template";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "jonringer";
    repo = pname;
    rev = "v${version}";
    sha256 = "1h6xdvhzg7nb0s82b3r5bsh8bfdb1l5sm7fa24lfwd396xp9yyig";
  };

  cargoSha256 = "0000000000000000000000000000000000000000000000000000";

  buildInputs = [ ];

  meta = with lib; {
    description = "Make creating nix expressions easy";
    homepage = "https://github.com/jonringer/nix-template/";
    license = licenses.cc0;
    maintainers = with maintainers; [ jonringer ];
  };
}
```

## Current Usage (Generically)

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package
$ nix-template python --pname requests -f pypi -l asl20
Creating directory: /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/default.nix
Please add the following line to the approriate file in top-level:

  requests = callPackage ../development/python-modules/requests { };
```
```nix
# pkgs/development/python-modules/requests/default.nix
{ lib, buildPythonPackage, fetchPypi }:

buildPythonPackage rec {
  pname = "requests";
  version = "0.0.1";

  src = fetchPypi {
    inherit pname version;
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
}
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
```

Other platforms, you'll need the following dependencies:
  - cargo
  - rustc
  - rust-clippy

## End Goal

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package
$ nix-template python --pname requests -f pypi -l asl20
Found latest stable release to be 2.24.0 on pypi.com
Creating directory: /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/default.nix
For an addition to nixpkgs as a python package, please add the following to pkgs/top-level/python-packages.nix:

  requests = callPackage ../development/python-modules/<PNAME> { };

For an addition to nixpkgs as a python application, please add the following to pkgs/top-level/all-packages.nix:

  requests = python3Packages.callPackage <PATH_FROM_CLI> { };
```
```nix
# pkgs/development/python-modules/requests/default.nix
{ lib, buildPythonPackage, fetchPypi }:

buildPythonPackage rec {
  pname = "requests";
  version = "2.24.0";

  src = fetchPypi {
    inherit pname version;
    sha256 = "b3559a131db72c33ee969480840fff4bb6dd111de7dd27c8ee1f820f4f00231b";
  };

  propagatedBuildInputs = [ ];

  pythonImportsCheck = [ "requests" ];

  meta = with lib; {
    description = "CHANGEME";
    homepage = "https://github.com/CHANGEME/requests/";
    license = licenses.asl20;
    maintainer = with maintainers; [ jonringer ];
  };
}
```


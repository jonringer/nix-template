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

## Current Usage (--from-url, github and pypi only)

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
{ lib
, rustPlatform
, fetchFromGitHub
}:

rustPlatform.buildRustPackage rec {
  pname = "nix-template";
  version = "0.3.0";

  src = fetchFromGitHub {
    owner = "jonringer";
    repo = pname;
    rev = "v${version}";
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
}
```

## Current Usage (Generically)

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package
$ nix-template python --nixpkgs --pname requests -f pypi -l asl20
Creating directory: /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/default.nix
Please add the following line to the approriate file in top-level:

  requests = callPackage ../development/python-modules/requests { };
```
```nix
# pkgs/development/python-modules/requests/default.nix
{ lib
, buildPythonPackage
, fetchPypi
}:

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
# or
nix develop
```

Other platforms, you'll need the following dependencies:
  - cargo
  - rustc
  - rust-clippy

## End Goal (Only better nixpkgs support missing)

```bash
# only need to config once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root /home/jon/projects/nixpkgs

# add a package
$ nix-template python --nixpkgs -u https://pypi.org/project/requests/
Determining latest release for requests
Creating directory: /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/
Generating python expression at /home/jon/projects/nixpkgs/pkgs/development/python-modules/requests/default.nix
For an addition to nixpkgs as a python package, please add the following to pkgs/top-level/python-packages.nix:

  requests = callPackage ../development/python-modules/requests { };

For an addition to nixpkgs as a python application, please add the following to pkgs/top-level/all-packages.nix:

  requests = with python3Packages; toPythonApplication requests { };
```
```nix
{ lib
, buildPythonPackage
, fetchPypi
, certifi
, charset-normalizer
, idna
, urllib3
}:

buildPythonPackage rec {
  pname = "requests";
  version = "2.28.1";

  src = fetchPypi {
    inherit pname version;
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
}
```


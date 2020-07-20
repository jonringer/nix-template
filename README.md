# Nix-template

*NOTE:* This is still very much WIP :)

Make creating nix expressions easy. Provide a nice way to create largely boilerplate nix-expressions.

## Roadmap

- [ ] Finalize cli semantics
- [ ] Ease usage with nixpkgs repo
- [ ] Support Language/frameworks/usage templates:
  - [ ] Python
  - [ ] Qt
  - [ ] Stdenv
  - [ ] Go
  - [ ] Haskell
  - [ ] mkShell
  - [ ] and many more...
- [ ] Add option (--comments?) to embed noob-friendly comments and explanations about common usage patterns
- [ ] Allow contributor information to be set locally (similar to git settings)
- [X] Implement shell completion (nix-template completions <SHELL>)

## End Goal

```bash
$ nix-template python -pname requests -f pypi pkgs/development/python-modules/
Generating python expression at pkgs/development/python-modules/requests/default.nix
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
  version = "<changeme>";

  src = fetchPypi {
    inherit pname version;
    sha256 = lib.fakeSha256;
  };

  pythonImportsCheck = [ "requests" ];

  meta = with lib; {
    description = "<changeme>";
    homepage = "https://github.com/<owner>/requests/";
    license = license.<changeme>;
    maintainer = with maintainers; [ <maintainer-name> ];
  };
}
```


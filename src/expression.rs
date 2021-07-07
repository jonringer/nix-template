use crate::types::{ExpressionInfo, Fetcher, Template};

fn derivation_helper(template: &Template) -> (String, String) {
    let (input, derivation, documentation_key): (&str, &str, Option<&str>) = match template {
        Template::stdenv  => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        Template::python  => ("buildPythonPackage", "buildPythonPackage", None),
        Template::mkshell => ("pkgs ? import <nixpkgs> {}", "with pkgs;\n\nmkShell", None),
        Template::qt      => ("mkDerivation", "mkDerivation", None),
        Template::go      => ("buildGoModule", "buildGoModule", None),
        Template::rust    => ("rustPlatform", "rustPlatform.buildRustPackage", None),
        Template::flake   => ("", "", None), // flakes aren't a normal expression
        Template::test    => ("", "", None), // Tests aren't a normal expression
    };

    match documentation_key {
        Some(key) => (String::from(input),
                      format!("@doc:{}@{}", key, derivation)),
        None => (String::from(input), String::from(derivation))
    }
}

fn fetch_block(fetcher: &Fetcher) -> (&'static str, &'static str) {
    match fetcher {
        Fetcher::github => (
            "fetchFromGitHub",
            "  @doc:fetcher@src = fetchFromGitHub {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::gitlab => (
            "fetchFromGitLab",
            "  @doc:fetcher@src = fetchFromGitLab {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::url => (
            "fetchurl",
            "  @doc:fetcher@src = fetchurl {
    url = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::zip => (
            "fetchzip",
            "  @doc:fetcher@src = fetchzip {
    url = \"CHANAGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::pypi => (
            "fetchPypi",
            "  @doc:fetcher@src = fetchPypi {
    inherit pname version;
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
    }
}

fn build_inputs(template: &Template) -> &'static str {
    match template {
        Template::python => "  @doc:buildDependencies@propagatedBuildInputs = [ ];

  pythonImportsCheck = [ \"@pname-import-check@\" ];",
        Template::rust => "  @doc:buildDependencies@cargoSha256 = \"0000000000000000000000000000000000000000000000000000\";

  buildInputs = [ ];",
        Template::go => "  @doc:buildDependencies@vendorSha256 = \"0000000000000000000000000000000000000000000000000000\";

  subPackages = [ \".\" ];",
        _ => "  buildInputs = [ ];",
    }
}

fn meta() -> &'static str {
        "
  @doc:meta@meta = with lib; {
    description = \"CHANGE\";
    homepage = \"https://github.com/CHANGE/@pname@/\";
    license = licenses.@license@;
    maintainers = with maintainers; [ @maintainer@ ];
  };"
}

pub fn generate_expression(info: &ExpressionInfo) -> String {
    match &info.template {
        Template::flake   => r#"{
  description = "CHANGEME";

  inputs = {
    utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs, utils }:
    let
      # put devShell and any other required packages into local overlay
      localOverlay = import ./nix/overlay.nix;

      pkgsForSystem = system: import nixpkgs {
        # if you have additional overlays, you may add them here
        overlays = [
          localOverlay # this should expose devShell
        ];
        inherit system;
      };
    # https://github.com/numtide/flake-utils#usage for more examples
    in utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" ] (system: rec {
      legacyPackages = pkgsForSystem system;
      packages = utils.lib.flattenTree {
        inherit (legacyPackages) devShell myPkg;
      };
      defaultPackage = packages.myPkg;
      apps.<mypkg> = utils.lib.mkApp { drv = packages.myPkg; };  # use as `nix run <mypkg>`
      hydraJobs = { inherit (legacyPackages) myPkg; };
      checks = { inherit (legacyPackages) myPkg; };              # items to be ran as part of `nix flake check`
  }) // {
    # non-system suffixed items should go here
    overlay = localOverlay;
    overlays = []; # list or attr set of overlays
    nixosModule = { config }: { options = {}; config = {};}; # export single module
    nixosModules = {}; # attr set or list
    nixosConfigurations.hostname = { config, pkgs }: {};
  };
}"#.to_string(),
        Template::test => r#"import ./make-test-python.nix ({ pkgs, ... }:
{
  name = "@pname@";
  meta = with pkgs.lib.maintainers; {
    maintainers = [ @maintainer@ ];
  };
  machine = { pkgs, ... }: {
    environment.systemPackages = [ @pname@ ];
    services.@pname@.enable = true;
    virtualisation.memorySize = 512;
  };

  testScript =
    ''
      start_all()

      machine.wait_for_unit("multi-user.target")
      machine.wait_for_unit("@pname@.service")
      machine.wait_for_open_port(8080)
      machine.succeed("CMD")
    '';
})"#.to_string(),
        Template::mkshell => "with import <nixpkgs> { };

mkShell rec {
  # include any libraries or programs in buildInputs
  buildInputs = [
  ];

  # shell commands to be ran upon entering shell
  shellHook = ''
  '';
}
"
        .to_string(),
        _ => {
            // Generate nix expression
            let (dh_input, dh_block) = derivation_helper(&info.template);
            let (f_input, f_block) = fetch_block(&info.fetcher);

            let inputs = [String::from("lib"), dh_input, f_input.to_string() ];

            let header = format!("{{ {input_list} }}:", input_list = inputs.join(", "));

            info.format(&format!(
                "{header}

{dh_helper} rec {{
  pname = \"{pname}\";
  version = \"{version}\";

{f_block}

{build_inputs}
{meta}
}}
",
                header = header,
                dh_helper = dh_block,
                pname = &info.pname,
                version = &info.version,
                f_block = f_block,
                build_inputs = build_inputs(&info.template),
                meta = if info.include_meta { meta() } else { "" },
            ))
        }
    }
}

fn calling_expression (template: Template) -> &'static str {
  match template {
    Template::test => "  {} = handleTest {} {{ }};",
    _ => "  {} = callPackage {} {{ }};"
  }
}
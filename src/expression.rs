use crate::types::{ExpressionInfo, Fetcher, Template};

fn derivation_helper(template: &Template) -> (String, String) {
    let (input, derivation, documentation_key): (&str, &str, Option<&str>) = match template {
        Template::stdenv => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        Template::python => ("buildPythonPackage", "buildPythonPackage", None),
        Template::mkshell => ("pkgs ? import <nixpkgs> {}", "with pkgs;\n\nmkShell", None),
        Template::qt => ("mkDerivation", "mkDerivation", None),
        Template::go => ("buildGoModule", "buildGoModule", None),
        Template::rust => ("rustPlatform", "rustPlatform.buildRustPackage", None),
        Template::test => ("", "", None),  // Tests aren't a normal expression
        Template::module => ("", "", None), // Modules aren't a normal expression
    };

    match documentation_key {
        Some(key) => (String::from(input), format!("@doc:{}@{}", key, derivation)),
        None => (String::from(input), String::from(derivation)),
    }
}

fn fetch_block(fetcher: &Fetcher) -> (&'static str, &'static str) {
    match fetcher {
        Fetcher::github => (
            "fetchFromGitHub",
            "  @doc:fetcher@src = fetchFromGitHub {
    owner = \"@owner@\";
    repo = pname;
    rev = @rev@;
    sha256 = \"@src_sha@\";
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
    url = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::pypi => (
            "fetchPypi",
            "  @doc:fetcher@src = fetchPypi {
    inherit pname version;
    sha256 = \"@src_sha@\";
  };",
        ),
    }
}

fn addtional_pkg_attr_headers(template: &Template) -> &'static str {
    match template {
        Template::python => "\n  @doc:pythonFormat@format = \"setuptools\";",
        _ => "",
    }
}

fn build_inputs(template: &Template) -> &'static str {
    match template {
        Template::python => "  @doc:buildDependencies@propagatedBuildInputs = [@propagated_build_inputs@ ];

  @doc:pythonImportsCheck@pythonImportsCheck = [ \"@pname-import-check@\" ];",
        Template::rust => "  @doc:buildDependencies@
  @doc:cargoSha256@cargoSha256 = \"0000000000000000000000000000000000000000000000000000\";

  buildInputs = [ ];",
        Template::go => "  @doc:buildDependencies@
  @doc:vendorSha256@vendorSha256 = \"0000000000000000000000000000000000000000000000000000\";

  @doc:goSubPackages@subPackages = [ \".\" ];",
        _ => "  buildInputs = [ ];",
    }
}

fn meta() -> &'static str {
    "
  @doc:meta@meta = with lib; {
    description = \"@description@\";
    homepage = \"@homepage@\";
    license = licenses.@license@;
    maintainers = with maintainers; [ @maintainer@ ];
  };"
}

pub fn generate_expression(info: &ExpressionInfo) -> String {
    match &info.template {
        Template::module   => r#"@doc:nixosModules@{ pkgs, lib, config, ... }:

with lib;

let
  cfg = config.services.@pname@;
in {
  options.services.@pname@ = {
    enable = mkEnableOption "CHANGE";

    package = mkOption {
      type = types.package;
      default = pkgs.@pname@;
      defaultText = "pkgs.@pname@";
      description = "Set version of @pname@ package to use.";
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ]; # if user should have the command available as well
    services.dbus.packages = [ cfg.package ]; # if the package has dbus related configuration

    systemd.services.@pname@ = {
      description = "@pname@ server daemon.";

      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ]; # if networking is needed

      restartIfChanged = true; # set to false, if restarting is problematic

      serviceConfig = {
        DynamicUser = true;
        ExecStart = "${cfg.package}/bin/@pname@";
        Restart = "always";
      };
    };
  };

  meta.maintainers = with lib.maintainers; [ @maintainer@ ];
}"#.to_owned(),
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
            let addtional_pkg_attr_headers = addtional_pkg_attr_headers(&info.template);

            let mut inputs = vec!(String::from("lib"), dh_input, f_input.to_string());
            inputs.extend(info.propagated_build_inputs.iter().map(|s| s.to_owned()));

            let header = format!("{{ {input_list}\n}}:", input_list = inputs.join("\n, "));

            info.format(&format!(
                "{header}

{dh_helper} rec {{
  pname = \"{pname}\";
  version = \"{version}\";{addtional_pkg_attr_headers}

{f_block}
@doc:buildPhases@
{build_inputs}
{meta}
}}
",
                header = header,
                dh_helper = dh_block,
                pname = &info.pname,
                version = &info.version,
                addtional_pkg_attr_headers = addtional_pkg_attr_headers,
                f_block = f_block,
                build_inputs = build_inputs(&info.template),
                meta = if info.include_meta { meta() } else { "" },
            ))
        }
    }
}

pub fn generate_flake_nix(template: &Template, output_file: &str, directory_name: &str) -> String {
    let inner_attr_path = if *template == Template::python {
        ".python3Packages"
    } else {
        ""
    };

    format!(r#"{{
  # This should be the directory name
  description = "{directory}";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  }};

  outputs =
    {{ self, nixpkgs, ... }}:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {{
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${{system}};
        in
        {{
          default = pkgs{inner_attr_path}.callPackage ./{output_file} {{ }};
        }}
      );
    }};
}}
"#, directory = directory_name, inner_attr_path = inner_attr_path, output_file = output_file)
}

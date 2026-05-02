use crate::types::{ExpressionInfo, Fetcher, Template};

fn derivation_helper(info: &ExpressionInfo) -> (String, String) {
    let (input, derivation, documentation_key): (&str, &str, Option<&str>) = match info.template {
        Template::stdenv => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        // For Python, switch between the library and application builders
        // depending on `info.python_application`.
        Template::python if info.python_application => {
            ("buildPythonApplication", "buildPythonApplication", None)
        }
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
        Fetcher::gitea => (
            "fetchFromGitea",
            "  @doc:fetcher@src = fetchFromGitea {
    domain = \"@domain@\";
    owner = \"@owner@\";
    repo = pname;
    rev = @rev@;
    sha256 = \"@src_sha@\";
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

fn build_inputs(info: &ExpressionInfo) -> String {
    match info.template {
        // Python applications don't carry a Python-import smoke test the way
        // libraries do; their entry points are exercised at runtime.
        Template::python if info.python_application =>
            "  @doc:buildDependencies@propagatedBuildInputs = [@propagated_build_inputs@ ];".to_owned(),
        Template::python => "  @doc:buildDependencies@propagatedBuildInputs = [@propagated_build_inputs@ ];

  @doc:pythonImportsCheck@pythonImportsCheck = [ \"@pname-import-check@\" ];".to_owned(),
        Template::rust => {
            // Conditionally render `nativeBuildInputs` only when inferred,
            // to keep the output tidy for projects without system deps.
            let native = if info.native_build_inputs.is_empty() {
                String::new()
            } else {
                "\n  nativeBuildInputs = [@native_build_inputs@ ];\n".to_owned()
            };
            format!(
                "  @doc:buildDependencies@
  @doc:cargoHash@cargoHash = \"@cargo_hash@\";
{native}
  buildInputs = [@build_inputs@ ];",
                native = native,
            )
        }
        Template::go => {
            // Mirror the Rust path: only emit nativeBuildInputs / buildInputs
            // attributes when CGO inference produced something. Empty
            // attributes would just be noise users have to delete.
            let native = if info.native_build_inputs.is_empty() {
                String::new()
            } else {
                "\n  nativeBuildInputs = [@native_build_inputs@ ];".to_owned()
            };
            let build = if info.build_inputs.is_empty() {
                String::new()
            } else {
                "\n  buildInputs = [@build_inputs@ ];".to_owned()
            };
            format!(
                "  @doc:buildDependencies@
  @doc:vendorHash@vendorHash = \"@vendor_hash@\";{native}{build}

  @doc:goSubPackages@subPackages = [ \".\" ];",
                native = native,
                build = build,
            )
        }
        _ => "  buildInputs = [ ];".to_owned(),
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
            let (dh_input, dh_block) = derivation_helper(info);
            let (f_input, f_block) = fetch_block(&info.fetcher);
            let addtional_pkg_attr_headers = addtional_pkg_attr_headers(&info.template);

            let mut inputs = vec!(String::from("lib"), dh_input, f_input.to_string());
            inputs.extend(info.propagated_build_inputs.iter().map(|s| s.to_owned()));
            // Inferred Rust system deps: surface each in the function
            // header so `callPackage` can pass them in. nativeBuildInputs
            // are listed first to mirror nixpkgs convention.
            inputs.extend(info.native_build_inputs.iter().map(|s| s.to_owned()));
            inputs.extend(info.build_inputs.iter().map(|s| s.to_owned()));

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
                build_inputs = build_inputs(info),
                meta = if info.include_meta { meta() } else { "" },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExpressionInfo, Fetcher, Template};

    fn rust_info() -> ExpressionInfo {
        ExpressionInfo {
            pname: "demo".to_owned(),
            version: "1.0.0".to_owned(),
            license: "mit".to_owned(),
            maintainer: "me".to_owned(),
            fetcher: Fetcher::github,
            template: Template::rust,
            path_to_write: std::path::PathBuf::new(),
            top_level_path: std::path::PathBuf::new(),
            include_documentation_links: false,
            include_meta: true,
            tag_prefix: "".to_owned(),
            owner: "demo".to_owned(),
            src_sha: "sha256-demo".to_owned(),
            description: "demo".to_owned(),
            homepage: "https://example.com".to_owned(),
            propagated_build_inputs: Vec::new(),
            cargo_hash: "sha256-cargo".to_owned(),
            vendor_hash: "sha256-vendor".to_owned(),
            domain: "".to_owned(),
            python_application: false,
            build_inputs: Vec::new(),
            native_build_inputs: Vec::new(),
        }
    }

    #[test]
    fn rust_without_inferred_deps_omits_native_build_inputs() {
        let info = rust_info();
        let expr = generate_expression(&info);
        let out = info.format(&expr);
        assert!(out.contains("buildInputs = [ ];"));
        assert!(
            !out.contains("nativeBuildInputs"),
            "should not render nativeBuildInputs when none inferred:\n{}",
            out
        );
    }

    #[test]
    fn rust_with_inferred_deps_renders_both_lists() {
        let mut info = rust_info();
        info.build_inputs = vec!["openssl".to_owned()];
        info.native_build_inputs = vec!["pkg-config".to_owned()];
        let expr = generate_expression(&info);
        let out = info.format(&expr);
        // format_inputs joins with `\n    ` and pads the closing
        // bracket with a space, so a single-entry list renders as
        // `[\n    name\n  ];` (note: two spaces before `];`).
        assert!(
            out.contains("nativeBuildInputs = [\n    pkg-config\n  ];"),
            "missing nativeBuildInputs in:\n{}",
            out
        );
        assert!(
            out.contains("buildInputs = [\n    openssl\n  ];"),
            "missing buildInputs in:\n{}",
            out
        );
        // The function-header should also list both crates so callPackage
        // can wire them through.
        assert!(out.contains(", pkg-config"), "header missing pkg-config: {}", out);
        assert!(out.contains(", openssl"), "header missing openssl: {}", out);
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

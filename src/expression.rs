use crate::types::{ExpressionInfo, Fetcher, Template};

fn derivation_helper(template: &Template) -> (String, String) {
    let (input, derivation, documentation_key): (&str, &str, Option<&str>) = match template {
        Template::stdenv  => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        Template::python  => ("buildPythonPackage", "buildPythonPackage", None),
        Template::mkshell => ("pkgs ? import <nixpkgs> {}", "with pkgs;\n\nmkShell", None),
        Template::qt      => ("mkDerivation", "mkDerivation", None),
        Template::go      => ("buildGoModule", "buildGoModule", None),
        Template::rust    => ("rustPlatform", "rustPlatform.buildRustPackage", None),
    };

    match documentation_key {
        Some(key) => (String::from(input),
                      format!("@documentation:{}@\n{}", key, derivation)),
        None => (String::from(input), String::from(derivation))
    }
}

fn fetch_block(fetcher: &Fetcher) -> (String, String) {
    let (input, block) = match fetcher {
        Fetcher::github => (
            "fetchFromGitHub",
            "  src = fetchFromGitHub {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::gitlab => (
            "fetchFromGitLab",
            "  src = fetchFromGitLab {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::url => (
            "fetchurl",
            "  src = fetchurl {
    url = \"CHANGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::zip => (
            "fetchzip",
            "  src = fetchzip {
    url = \"CHANAGE\";
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
        Fetcher::pypi => (
            "fetchPypi",
            "  src = fetchPypi {
    inherit pname version;
    sha256 = \"0000000000000000000000000000000000000000000000000000\";
  };",
        ),
    };

    (String::from(input),
     format!("  @documentation:fetcher@\n{}", block))
}

fn build_inputs(template: &Template) -> String {
    let build_inputs = match template {
        Template::python => "  propagatedBuildInputs = [ ];

  pythonImportsCheck = [ \"@pname@\" ];",
        Template::rust => "  cargoSha256 = \"0000000000000000000000000000000000000000000000000000\";

  buildInputs = [ ];",
        Template::go => "  vendorSha256 = \"0000000000000000000000000000000000000000000000000000\";

  subPackages = [ \".\" ];",
        _ => "  buildInputs = [ ];",
    };

    format!("  @documentation:buildDependencies@\n{}", build_inputs)
}

fn meta() -> &'static str {
        "
  @documentation:meta@
  meta = with lib; {
    description = \"CHANGE\";
    homepage = \"https://github.com/CHANGE/@pname@/\";
    license = licenses.@license@;
    maintainers = with maintainers; [ @maintainer@ ];
  };"
}

pub fn generate_expression(info: &ExpressionInfo) -> String {
    match &info.template {
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

            let inputs = [String::from("lib"), dh_input, f_input];

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

use crate::types::Fetcher;
use crate::types::Template;

fn derivation_helper(template: &Template) -> (&'static str, &'static str) {
    match template {
        Template::stdenv => ("stdenv", "stdenv.mkDerivation"),
        Template::python => ("buildPythonPackage", "buildPythonPackage"),
        Template::mkshell => ("pkgs ? import <nixpkgs> {}", "with pkgs;\n\nmkShell"),
    }
}

fn fetch_block(fetcher: &Fetcher) -> (&'static str, &'static str) {
    match fetcher {
        Fetcher::github => (
            "fetchFromGitHub",
            "  src = fetchFromGitHub {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = lib.fakeSha256;
  };",
        ),
        Fetcher::gitlab => (
            "fetchFromGitLab",
            "  src = fetchFromGitLab {
    owner = \"CHANGE\";
    repo = pname;
    rev = \"CHANGE\";
    sha256 = lib.fakeSha256;
  };",
        ),
        Fetcher::url => (
            "fetchurl",
            "  src = fetchurl {
    url = \"CHANGE\";
    sha256 = lib.fakeSha256;
  };",
        ),
        Fetcher::zip => (
            "fetchzip",
            "  src = fetchzip {
    url = \"CHANAGE\";
    sha256 = lib.fakeSha256;
  };",
        ),
        Fetcher::pypi => (
            "fetchPypi",
            "  src = fetchPypi {
    inherit pname version;
    sha256 = lib.fakeSha256;
  };",
        ),
    }
}

fn build_inputs(template: &Template) -> &'static str {
    match template {
        Template::python => "  propagatedBuildInputs = [ ];",
        _ => "  buildInputs = [ ];",
    }
}

fn meta(template: &Template, fetcher: &Fetcher, maintainer: &str) -> String {
    format!(
"  meta = with lib; {{
    description = \"CHANGE\";
    homepage = \"CHANGE\";
    license = license.CHANGE;
    maintainer = with maintainers; [ {maintainer} ];
  }}", maintainer=maintainer)
}

pub fn generate_expression(template: &Template, fetcher: &Fetcher, pname: &str, version: &str, maintainer: &str) -> String {
    match template {
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
            let (dh_input, dh_block) = derivation_helper(&template);
            let (f_input, f_block) = fetch_block(&fetcher);

            let inputs = &["lib", dh_input, f_input];

            let header = format!("{{ {input_list} }}:", input_list = inputs.join(", "));

            format!(
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
                pname = pname,
                version = version,
                f_block = f_block,
                build_inputs = build_inputs(&template),
                meta = meta(&template, &fetcher, &maintainer)
            )
        }
    }
}

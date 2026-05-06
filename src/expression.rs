use crate::types::{ExpressionInfo, Fetcher, Template};

fn derivation_helper(info: &ExpressionInfo) -> (String, String) {
    let (input, derivation, documentation_key): (&str, &str, Option<&str>) = match info.template {
        Template::auto => unreachable!("'auto' template should be resolved before expression generation"),
        Template::stdenv => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        Template::stdenvNoCC => (
            "stdenvNoCC",
            "stdenvNoCC.mkDerivation",
            Some("stdenvNoCCMkDerivation"),
        ),
        // Python library packages use buildPythonPackage
        Template::python_package => {
            ("buildPythonPackage", "buildPythonPackage", None)
        }
        // Python applications use buildPythonApplication
        Template::python_application => {
            ("buildPythonApplication", "buildPythonApplication", None)
        }
        Template::mkshell => ("pkgs ? import <nixpkgs> {}", "with pkgs;\n\nmkShell", None),
        Template::go => ("buildGoModule", "buildGoModule", None),
        Template::rust => ("rustPlatform", "rustPlatform.buildRustPackage", None),
        Template::npm => ("buildNpmPackage", "buildNpmPackage", Some("buildNpmPackage")),
        Template::pnpm => ("stdenv", "stdenv.mkDerivation", Some("stdenvMkDerivation")),
        Template::dotnet => ("buildDotnetModule", "buildDotnetModule", Some("buildDotnetModule")),
        Template::ruby => ("bundlerApp", "bundlerApp", Some("bundlerApp")),
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
    repo = finalAttrs.pname;
    rev = @rev@;
    sha256 = \"@src_sha@\";
  };",
        ),
        Fetcher::gitlab => (
            "fetchFromGitLab",
            "  @doc:fetcher@src = fetchFromGitLab {
    owner = \"@owner@\";
    repo = finalAttrs.pname;
    rev = @rev@;
    sha256 = \"@src_sha@\";
  };",
        ),
        Fetcher::gitea => (
            "fetchFromGitea",
            "  @doc:fetcher@src = fetchFromGitea {
    domain = \"@domain@\";
    owner = \"@owner@\";
    repo = finalAttrs.pname;
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
    inherit (finalAttrs) pname version;
    sha256 = \"@src_sha@\";
  };",
        ),
        Fetcher::local => (
            "",
            "  @doc:fetcher@src = ./..;",
        ),
    }
}

fn addtional_pkg_attr_headers(template: &Template) -> &'static str {
    match template {
        Template::python_package | Template::python_application => {
            "\n  @doc:pythonFormat@format = \"setuptools\";"
        }
        _ => "",
    }
}

fn build_inputs(info: &ExpressionInfo) -> String {
    match info.template {
        // Python applications don't carry a Python-import smoke test the way
        // libraries do; their entry points are exercised at runtime.
        Template::python_application =>
            "  @doc:buildDependencies@propagatedBuildInputs = [@propagated_build_inputs@ ];".to_owned(),
        // Python packages (libraries) include pythonImportsCheck for smoke testing
        Template::python_package => "  @doc:buildDependencies@propagatedBuildInputs = [@propagated_build_inputs@ ];

  @doc:pythonImportsCheck@pythonImportsCheck = [ \"@pname-import-check@\" ];".to_owned(),
        Template::rust => {
            // Conditionally render `nativeBuildInputs` only when inferred,
            // to keep the output tidy for projects without system deps.
            let native = if info.native_build_inputs.is_empty() {
                String::new()
            } else {
                "\n\n  nativeBuildInputs = [@native_build_inputs@ ];".to_owned()
            };
            format!(
                "  @doc:cargoHash@cargoHash = \"@cargo_hash@\";{native}

  @doc:buildDependencies@buildInputs = [@build_inputs@ ];",
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
        Template::npm => {
            "  npmDepsHash = \"@npm_deps_hash@\";".to_owned()
        }
        Template::pnpm => {
            "  nativeBuildInputs = [
    nodejs
    pnpmConfigHook
    pnpm_10
  ];

  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    fetcherVersion = 3;
    hash = \"@pnpm_deps_hash@\";
  };".to_owned()
        }
        Template::dotnet => {
            "  projectFile = \"@project_file@\";\n  nugetDeps = ./deps.json;  # Run `nix-build -A package-name.passthru.fetch-deps` to generate".to_owned()
        }
        Template::ruby => {
            // Conditionally render build inputs only when inferred
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
                "  gemdir = ./.;\n  exes = [ \"@pname@\" ];  # To build this package, you need Gemfile, Gemfile.lock, and gemset.nix in this directory{native}{build}\n",
                native = native,
                build = build,
            )
        }
        // stdenv / stdenvNoCC: render `nativeBuildInputs` only when
        // populated (via --native-build-inputs); always render
        // `buildInputs` to preserve the existing template's ergonomic
        // placeholder when no inputs are supplied.
        _ => {
            let native = if info.native_build_inputs.is_empty() {
                String::new()
            } else {
                "  nativeBuildInputs = [@native_build_inputs@ ];\n\n".to_owned()
            };
            format!("{native}  buildInputs = [@build_inputs@ ];", native = native)
        }
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
        Template::auto => unreachable!("'auto' template should be resolved before expression generation"),
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
        Template::ruby => {
            let (_, dh_block) = derivation_helper(info);
            let meta_content = if info.include_meta { meta() } else { "" };

            let mut inputs = vec![String::from("lib"), String::from("bundlerApp")];

            // Add inferred system dependencies to function header
            inputs.extend(info.native_build_inputs.iter().map(|s| s.to_owned()));
            inputs.extend(info.build_inputs.iter().map(|s| s.to_owned()));

            let input_list = inputs.join("\n, ");
            let header = format!("{{ {input_list}\n}}:", input_list = input_list);

            info.format(&format!(
                "{header}

{dh_helper} {{
  pname = \"{pname}\";
{build_inputs}{meta}
}}
",
                header = header,
                dh_helper = dh_block,
                pname = &info.pname,
                build_inputs = build_inputs(info),
                meta = meta_content,
            ))
        }
        _ => {
            // Generate nix expression
            let (dh_input, dh_block) = derivation_helper(info);
            let (f_input, f_block) = fetch_block(&info.fetcher);
            let addtional_pkg_attr_headers = addtional_pkg_attr_headers(&info.template);

            let mut inputs = vec!(String::from("lib"), dh_input);
            // Only add fetcher input if it's not empty (local fetcher has no input)
            if !f_input.is_empty() {
                inputs.push(f_input.to_string());
            }
            inputs.extend(info.propagated_build_inputs.iter().map(|s| s.to_owned()));

            // pnpm template needs special inputs for fetchPnpmDeps and pnpm setup
            if info.template == Template::pnpm {
                inputs.push("fetchPnpmDeps".to_string());
                inputs.push("nodejs".to_string());
                inputs.push("pnpm_10".to_string());
                inputs.push("pnpmConfigHook".to_string());
            }

            // Inferred / user-supplied system deps: surface each in the
            // function header so `callPackage` can pass them in.
            // nativeBuildInputs are listed first to mirror the nixpkgs
            // convention.
            inputs.extend(info.native_build_inputs.iter().map(|s| s.to_owned()));
            inputs.extend(info.build_inputs.iter().map(|s| s.to_owned()));

            // A single attribute may legitimately appear in BOTH
            // buildInputs and nativeBuildInputs (e.g. `protobuf` is
            // commonly both a build-time tool and a runtime library).
            // The function header still has to list it exactly once,
            // since duplicate function arguments are a Nix syntax error.
            // Preserve the order of first appearance.
            let mut seen = std::collections::HashSet::new();
            inputs.retain(|s| seen.insert(s.clone()));

            let header = format!("{{ {input_list}\n}}:", input_list = inputs.join("\n, "));

            info.format(&format!(
                "{header}

{dh_helper} (finalAttrs: {{
  pname = \"{pname}\";
  version = \"{version}\";{addtional_pkg_attr_headers}

{f_block}

{build_inputs}
{meta}
}})
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
            npm_deps_hash: "sha256-npm".to_owned(),
            pnpm_deps_hash: "sha256-pnpm".to_owned(),
            project_file: "Project.csproj".to_owned(),
            domain: "".to_owned(),
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

    fn stdenv_no_cc_info() -> ExpressionInfo {
        ExpressionInfo {
            pname: "myfont".to_owned(),
            version: "1.0.0".to_owned(),
            license: "ofl".to_owned(),
            maintainer: "me".to_owned(),
            fetcher: Fetcher::github,
            template: Template::stdenvNoCC,
            path_to_write: std::path::PathBuf::new(),
            top_level_path: std::path::PathBuf::new(),
            include_documentation_links: false,
            include_meta: true,
            tag_prefix: "".to_owned(),
            owner: "myfont".to_owned(),
            src_sha: "sha256-demo".to_owned(),
            description: "demo font".to_owned(),
            homepage: "https://example.com".to_owned(),
            propagated_build_inputs: Vec::new(),
            cargo_hash: "".to_owned(),
            vendor_hash: "".to_owned(),
            npm_deps_hash: "".to_owned(),
            pnpm_deps_hash: "".to_owned(),
            project_file: "".to_owned(),
            domain: "".to_owned(),
            build_inputs: Vec::new(),
            native_build_inputs: Vec::new(),
        }
    }

    #[test]
    fn shared_attr_dedupes_in_function_header() {
        // The Nix language disallows duplicate function arguments, so an
        // attribute that legitimately appears in both buildInputs and
        // nativeBuildInputs (a common case for `protobuf`) must show up
        // exactly once in the function header — even though both list
        // bodies still mention it.
        let mut info = rust_info();
        info.build_inputs = vec!["protobuf".to_owned(), "openssl".to_owned()];
        info.native_build_inputs = vec!["protobuf".to_owned(), "pkg-config".to_owned()];
        let expr = generate_expression(&info);
        let out = info.format(&expr);

        let header_end = out.find("}:").expect("function header");
        let header = &out[..header_end];
        let occurrences = header.matches("protobuf").count();
        assert_eq!(
            occurrences, 1,
            "protobuf must appear exactly once in the function header, got {} in:\n{}",
            occurrences, header
        );

        // It should still appear in BOTH list bodies, however. The list
        // bodies preserve the user's insertion order rather than being
        // sorted, so match against the as-given sequences.
        let body = &out[header_end..];
        assert!(
            body.contains("nativeBuildInputs = [\n    protobuf\n    pkg-config\n  ];"),
            "missing dedup'd nativeBuildInputs body in:\n{}",
            body
        );
        assert!(
            body.contains("buildInputs = [\n    protobuf\n    openssl\n  ];"),
            "missing buildInputs body in:\n{}",
            body
        );
    }

    #[test]
    fn stdenv_with_user_supplied_inputs_renders_native_section() {
        // For non-rust/go templates the user can populate the input lists
        // via --build-inputs / --native-build-inputs. Both sections must
        // render and both attrs must surface in the function header.
        let mut info = rust_info();
        info.template = Template::stdenv;
        info.build_inputs = vec!["zlib".to_owned()];
        info.native_build_inputs = vec!["pkg-config".to_owned()];
        let expr = generate_expression(&info);
        let out = info.format(&expr);
        assert!(
            out.contains("nativeBuildInputs = [\n    pkg-config\n  ];"),
            "missing nativeBuildInputs in:\n{}",
            out
        );
        assert!(
            out.contains("buildInputs = [\n    zlib\n  ];"),
            "missing buildInputs in:\n{}",
            out
        );
        assert!(out.contains(", pkg-config"), "header missing pkg-config: {}", out);
        assert!(out.contains(", zlib"), "header missing zlib: {}", out);
    }

    #[test]
    fn stdenv_with_no_inputs_keeps_empty_buildinputs_placeholder() {
        // Existing behaviour: stdenv expressions render
        // `buildInputs = [ ];` as a placeholder when nothing's supplied.
        // No nativeBuildInputs section unless inferred/asked for.
        let mut info = rust_info();
        info.template = Template::stdenv;
        info.build_inputs = Vec::new();
        info.native_build_inputs = Vec::new();
        let expr = generate_expression(&info);
        let out = info.format(&expr);
        assert!(out.contains("buildInputs = [ ];"), "missing placeholder in:\n{}", out);
        assert!(
            !out.contains("nativeBuildInputs"),
            "should not render nativeBuildInputs without input:\n{}",
            out
        );
    }

    #[test]
    fn stdenv_no_cc_renders_stdenv_no_cc_mk_derivation() {
        // The whole point of the stdenvNoCC template is that it should
        // emit `stdenvNoCC.mkDerivation` (not `stdenv.mkDerivation`) and
        // surface `stdenvNoCC` in the function header.
        let info = stdenv_no_cc_info();
        let expr = generate_expression(&info);
        let out = info.format(&expr);
        assert!(
            out.contains("stdenvNoCC.mkDerivation"),
            "expected stdenvNoCC.mkDerivation in:\n{}",
            out
        );
        assert!(
            !out.contains("stdenv.mkDerivation"),
            "should not emit plain stdenv.mkDerivation:\n{}",
            out
        );
        // The function header should list stdenvNoCC, not stdenv.
        assert!(
            out.contains(", stdenvNoCC"),
            "header missing stdenvNoCC:\n{}",
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

/// Boilerplate `npins/default.nix` produced by `npins init`. Vendored
/// verbatim from upstream (MIT licensed). Embedded with `include_str!`
/// to avoid Rust string-escaping hazards.
pub const NPINS_DEFAULT_NIX: &str = include_str!("templates/npins_default.nix");

/// Empty `npins/sources.json` matching `npins init --bare` output.
/// Format version 7 is what npins/default.nix expects to read.
pub const NPINS_EMPTY_SOURCES_JSON: &str = "{\n  \"pins\": {},\n  \"version\": 7\n}\n";

/// Return the vendored npins/default.nix lockfile reader.
pub fn generate_npins_default_nix() -> &'static str {
    NPINS_DEFAULT_NIX
}

/// Return an empty npins/sources.json (no pins yet).
pub fn generate_npins_sources_json() -> &'static str {
    NPINS_EMPTY_SOURCES_JSON
}

/// Generate the wrapper `default.nix` placed alongside `npins/`. It
/// imports the vendored npins lockfile reader, pulls `nixpkgs` from it,
/// and `callPackage`s the package expression.
///
/// `package_file` is the basename of the package expression on disk
/// (e.g. `package.nix` or `default.nix`). Mirrors the python carve-out
/// from `generate_flake_nix` so that python packages resolve through
/// `pkgs.python3Packages.callPackage`.
pub fn generate_npins_wrapper_default_nix(template: &Template, package_file: &str) -> String {
    let inner_attr_path = if *template == Template::python_package || *template == Template::python_application {
        ".python3Packages"
    } else {
        ""
    };

    format!(
        r#"# Wrapper generated by nix-template --init-npins.
#
# Run `npins add channel nixpkgs-unstable` (or another channel/source)
# inside this directory to populate npins/sources.json. Once nixpkgs is
# pinned, `nix-build` here will build the package.
let
  sources = import ./npins;
  pkgs = import sources.nixpkgs {{ }};
in
pkgs{inner_attr_path}.callPackage ./{package_file} {{ }}
"#,
        inner_attr_path = inner_attr_path,
        package_file = package_file,
    )
}

pub fn generate_flake_nix(template: &Template, output_file: &str, directory_name: &str) -> String {
    let inner_attr_path = if *template == Template::python_package || *template == Template::python_application {
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

/// Generate the standardized `nix/overlay.nix` file. The overlay calls
/// `callPackage` against `./pkgs/<pname>/package.nix` (or
/// `python3Packages.callPackage` for python templates) so that consumers
/// can `pkgs.extend (import ./nix/overlay.nix)` from a `default.nix`,
/// `flake.nix`, or `release.nix`.
///
/// The list is rendered explicitly so users can see — and edit — which
/// packages are exposed. New packages are added by appending another
/// `final.callPackage ./package.nix { };` line.
pub fn generate_overlay_nix(template: &Template, pname: &str) -> String {
    if *template == Template::module {
        // Module-only projects don't have a package to expose. Emit an
        // empty overlay scaffold the user can fill in later.
        return r#"# Overlay generated by nix-template.
#
# Apply with `pkgs.extend (import ./nix/overlay.nix)` from a default.nix,
# flake.nix, or release.nix. Add packages by appending callPackage lines
# below.
final: prev: {
  # myPackage = final.callPackage ./package.nix { };
}
"#
        .to_string();
    }

    let call_package = match template {
        Template::python_package | Template::python_application => {
            "final.python3Packages.callPackage"
        }
        _ => "final.callPackage",
    };

    format!(
        r#"# Overlay generated by nix-template.
#
# Apply with `pkgs.extend (import ./nix/overlay.nix)` from a default.nix,
# flake.nix, or release.nix. Add packages by appending callPackage lines
# below.
final: prev: {{
  {pname} = {call_package} ./package.nix {{ }};
}}
"#,
        pname = pname,
        call_package = call_package,
    )
}

/// Generate the top-level `default.nix` for non-flake consumers. When
/// `with_npins` is true the wrapper imports nixpkgs from `./npins`;
/// otherwise it falls back to the `<nixpkgs>` channel. The wrapper
/// applies `./nix/overlay.nix` and exposes the package attribute so that
/// `nix-build` from the project root just works.
pub fn generate_structured_default_nix(template: &Template, pname: &str, with_npins: bool) -> String {
    let nixpkgs_import = if with_npins {
        r#"let
  sources = import ./npins;
  pkgs = (import sources.nixpkgs { }).extend (import ./nix/overlay.nix);
in"#
    } else {
        r#"let
  pkgs = (import <nixpkgs> { }).extend (import ./nix/overlay.nix);
in"#
    };

    if *template == Template::module {
        // Module-only project: there is no package to surface. Expose
        // the overlay-extended nixpkgs so consumers can pull whatever
        // they need.
        return format!(
            r#"# Top-level default.nix generated by nix-template.
#
# Build with `nix-build` from the project root. The package(s) defined in
# nix/overlay.nix are exposed via the overlay-extended nixpkgs.
{nixpkgs_import}
pkgs
"#,
            nixpkgs_import = nixpkgs_import,
        );
    }

    let attr_path = if *template == Template::python_package || *template == Template::python_application {
        format!("pkgs.python3Packages.{}", pname)
    } else {
        format!("pkgs.{}", pname)
    };

    format!(
        r#"# Top-level default.nix generated by nix-template.
#
# Build with `nix-build` from the project root. The overlay in
# nix/overlay.nix is applied to nixpkgs so that `{attr_path}` resolves to
# the locally defined package.
{nixpkgs_import}
{attr_path}
"#,
        nixpkgs_import = nixpkgs_import,
        attr_path = attr_path,
    )
}

/// Generate a flake.nix that references the standardized `./nix/overlay.nix`
/// rather than a sibling package file. Pairs with the structured layout
/// produced by `--init-flake` (with nix/ layout) or `--init-npins`.
pub fn generate_structured_flake_nix(template: &Template, pname: &str, directory_name: &str) -> String {
    if *template == Template::module {
        // Module-only flake: expose nixosModules instead of packages.
        return format!(
            r#"{{
  # This should be the directory name
  description = "{directory}";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  }};

  outputs =
    {{ self, nixpkgs, ... }}:
    {{
      nixosModules.{pname} = import ./nix/modules/{pname};
      nixosModules.default = self.nixosModules.{pname};
    }};
}}
"#,
            directory = directory_name,
            pname = pname,
        );
    }

    let attr_path = match template {
        Template::python_package | Template::python_application => {
            format!("overlayed.python3Packages.{}", pname)
        }
        _ => format!("overlayed.{}", pname),
    };

    format!(
        r#"{{
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
      overlays.default = import ./nix/overlay.nix;

      packages = forAllSystems (
        system:
        let
          overlayed = (import nixpkgs {{ inherit system; }}).extend self.overlays.default;
        in
        {{
          {pname} = {attr_path};
          default = self.packages.${{system}}.{pname};
        }}
      );
    }};
}}
"#,
        directory = directory_name,
        pname = pname,
        attr_path = attr_path,
    )
}

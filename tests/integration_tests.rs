use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test basic Python template generation without --flake-init
/// This should generate only a default.nix file
#[test]
fn test_python_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "-s", // --stdout flag
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Python package derivation
    assert!(stdout.contains("buildPythonPackage"));
    assert!(stdout.contains("fetchPypi"));
    assert!(stdout.contains("format = \"setuptools\""));
    assert!(stdout.contains("propagatedBuildInputs"));
    assert!(stdout.contains("pythonImportsCheck"));

    // Snapshot the output
    insta::assert_snapshot!("python_basic_template", stdout);
}

/// Test Python template generation WITH --init-flake (no PATH given).
/// This now produces the structured nix/ layout: package under
/// nix/pkgs/<pname>/package.nix, an overlay that uses
/// python3Packages.callPackage, and a flake.nix exposing the package.
///
/// NOTE: This test is disabled because --init-flake is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_python_template_with_flake_init() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "-s", // --stdout flag
            "--init-flake",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // All structured-layout artefacts should appear in stdout, separated by markers.
    assert!(stdout.contains("# ===== flake.nix ====="));
    assert!(stdout.contains("# ===== nix/overlay.nix ====="));

    // stdout ordering from main.rs is: package → flake → overlay.
    let after_flake_marker: Vec<&str> = stdout.split("# ===== flake.nix =====").collect();
    assert_eq!(after_flake_marker.len(), 2, "Expected a flake.nix marker");
    let package_nix = after_flake_marker[0].trim();

    let after_overlay_marker: Vec<&str> = after_flake_marker[1]
        .split("# ===== nix/overlay.nix =====")
        .collect();
    assert_eq!(after_overlay_marker.len(), 2, "Expected an overlay marker");
    let flake_nix = after_overlay_marker[0].trim();
    let overlay_nix = after_overlay_marker[1].trim();

    // Verify package part
    assert!(package_nix.contains("buildPythonPackage"));
    assert!(package_nix.contains("fetchPypi"));

    // Overlay must use python3Packages.callPackage for python templates.
    assert!(
        overlay_nix.contains("requests = final.python3Packages.callPackage ./package.nix"),
        "Python overlay should use python3Packages.callPackage; got:\n{}",
        overlay_nix
    );

    // Verify flake part — structured flake exposes overlays.default and the
    // python package via the overlayed pkgs set.
    assert!(flake_nix.contains("description ="));
    assert!(flake_nix.contains("inputs"));
    assert!(flake_nix.contains("nixpkgs.url"));
    assert!(flake_nix.contains("outputs"));
    assert!(
        flake_nix.contains("overlays.default = import ./nix/overlay.nix"),
        "structured flake should expose overlays.default; got:\n{}",
        flake_nix
    );
    assert!(
        flake_nix.contains("overlayed.python3Packages.requests"),
        "Python flake should resolve via python3Packages on overlayed pkgs; got:\n{}",
        flake_nix
    );
    assert!(flake_nix.contains("supportedSystems"));

    // Snapshot all three parts
    insta::assert_snapshot!("python_with_flake_package", package_nix);
    insta::assert_snapshot!("python_with_flake_overlay", overlay_nix);
    insta::assert_snapshot!("python_with_flake_flake", flake_nix);
}

/// Test Python template with explicit PyPI fetcher
/// This verifies that -f pypi works correctly
#[test]
fn test_python_template_pypi_fetcher_explicit() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-f",
            "pypi", // Explicitly specify PyPI fetcher
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify fetchPypi is used
    assert!(stdout.contains("fetchPypi"));
    assert!(
        !stdout.contains("fetchFromGitHub"),
        "Should not use GitHub fetcher"
    );

    insta::assert_snapshot!("python_pypi_explicit_package", stdout);
}

/// Test Python template with GitHub fetcher override
/// This verifies that we can override the default PyPI fetcher with GitHub.
#[test]
fn test_python_template_github_fetcher_override() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-f",
            "github", // Override with GitHub fetcher
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify fetchFromGitHub is used instead of fetchPypi
    assert!(stdout.contains("fetchFromGitHub"));
    assert!(!stdout.contains("fetchPypi"), "Should not use PyPI fetcher");
    assert!(stdout.contains("buildPythonPackage"));

    insta::assert_snapshot!("python_github_override_package", stdout);
}

/// Test Python template file writing (not just stdout)
/// `--init-flake` without an explicit PATH now uses the structured nix/
/// layout, so files land at nix/pkgs/<pname>/package.nix, nix/overlay.nix,
/// and flake.nix at the top. No top-level default.nix is emitted in this
/// mode (it's only added by --init-npins).
///
/// NOTE: This test is disabled because --init-flake is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_python_template_file_writing_with_flake() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "python_package",
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "--init-flake",
            // No -s flag, so it will write files
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify the structured layout was created.
    let package_nix_path = temp_path.join("nix/package.nix");
    let overlay_nix_path = temp_path.join("nix/overlay.nix");
    let flake_nix_path = temp_path.join("flake.nix");
    let top_default_nix_path = temp_path.join("default.nix");

    assert!(
        package_nix_path.exists(),
        "nix/package.nix should be created"
    );
    assert!(
        overlay_nix_path.exists(),
        "nix/overlay.nix should be created"
    );
    assert!(flake_nix_path.exists(), "flake.nix should be created");
    assert!(
        !top_default_nix_path.exists(),
        "top-level default.nix should NOT be created for --init-flake alone"
    );

    // Read and verify contents.
    let package_nix_content = std::fs::read_to_string(&package_nix_path).unwrap();
    let overlay_nix_content = std::fs::read_to_string(&overlay_nix_path).unwrap();
    let mut flake_nix_content = std::fs::read_to_string(&flake_nix_path).unwrap();

    assert!(package_nix_content.contains("buildPythonPackage"));
    assert!(
        overlay_nix_content.contains("python3Packages.callPackage"),
        "overlay should wire python3Packages.callPackage"
    );
    assert!(
        flake_nix_content.contains("overlays.default = import ./nix/overlay.nix"),
        "flake should expose overlays.default importing the overlay"
    );
    assert!(
        flake_nix_content.contains("overlayed.python3Packages.requests"),
        "flake should resolve the package via overlayed.python3Packages"
    );

    // Normalize the temp directory name in the description field for snapshot
    // stability — the description tracks the directory name which is random
    // for temp dirs.
    let temp_dir_name = temp_path.file_name().unwrap().to_str().unwrap();
    flake_nix_content = flake_nix_content.replace(
        &format!("description = \"{}\";", temp_dir_name),
        "description = \"<temp_dir>\";",
    );

    // Snapshot the file contents
    insta::assert_snapshot!("python_file_write_package", package_nix_content);
    insta::assert_snapshot!("python_file_write_overlay", overlay_nix_content);
    insta::assert_snapshot!("python_file_write_flake", flake_nix_content);
}

/// Test --init-npins to stdout: emits the structured nix/ layout —
/// package.nix under nix/pkgs/<pname>/, an overlay.nix, a top-level
/// default.nix, plus the npins/ scaffold.
///
/// NOTE: This test is disabled because --init-npins is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_init_npins_stdout() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "stdenv",
            "-p",
            "hello",
            "-v",
            "1.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s",
            "--init-npins",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Markers for each artefact in the structured layout.
    assert!(stdout.contains("# ===== nix/overlay.nix ====="));
    assert!(stdout.contains("# ===== default.nix ====="));
    assert!(stdout.contains("# ===== npins/default.nix ====="));
    assert!(stdout.contains("# ===== npins/sources.json ====="));

    // Top-level default.nix imports the overlay and pulls pkgs from npins.
    assert!(stdout.contains("sources = import ./npins;"));
    assert!(stdout.contains("overlays = [ (import ./nix/overlay.nix) ];"));
    assert!(stdout.contains("pkgs.hello"));

    // Overlay calls callPackage on the package under nix/pkgs/.
    assert!(stdout.contains("hello = final.callPackage ./pkgs/hello/package.nix { }"));

    // Empty pins lockfile, version 7
    assert!(stdout.contains("\"pins\": {}"));
    assert!(stdout.contains("\"version\": 7"));

    // Vendored npins boilerplate signature
    assert!(stdout.contains("mkFunctor"));
    assert!(stdout.contains("Unsupported format version"));
}

/// Test --init-npins file writing: scaffolds the structured nix/ layout
/// with the package under nix/pkgs/<pname>/package.nix, an overlay, a
/// top-level default.nix wrapper, and the npins/ directory.
///
/// NOTE: This test is disabled because --init-npins is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_init_npins_writes_three_files_and_renames() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "stdenv",
            "-p",
            "hello",
            "-v",
            "1.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "--init-npins",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Package lives under nix/pkgs/<pname>/package.nix
    let package_nix = temp_path
        .join("nix")
        .join("pkgs")
        .join("hello")
        .join("package.nix");
    assert!(
        package_nix.exists(),
        "nix/pkgs/hello/package.nix should be created"
    );

    // Top-level default.nix wraps the overlay, replacing the legacy npins
    // wrapper that used to live alongside the package file.
    let wrapper = temp_path.join("default.nix");
    assert!(wrapper.exists(), "top-level default.nix should be created");
    let wrapper_content = std::fs::read_to_string(&wrapper).unwrap();
    assert!(wrapper_content.contains("sources = import ./npins;"));
    assert!(wrapper_content.contains("overlays = [ (import ./nix/overlay.nix) ];"));
    assert!(wrapper_content.contains("pkgs.hello"));

    // Overlay calls callPackage on the new package path.
    let overlay = temp_path.join("nix").join("overlay.nix");
    assert!(overlay.exists(), "nix/overlay.nix should be created");
    let overlay_content = std::fs::read_to_string(&overlay).unwrap();
    assert!(
        overlay_content.contains("hello = final.callPackage ./pkgs/hello/package.nix { }"),
        "overlay should reference nix/pkgs/hello/package.nix; got:\n{}",
        overlay_content
    );

    // npins/ scaffold exists at project root.
    let npins_default = temp_path.join("npins").join("default.nix");
    let npins_sources = temp_path.join("npins").join("sources.json");
    assert!(
        npins_default.exists(),
        "npins/default.nix should be created"
    );
    assert!(
        npins_sources.exists(),
        "npins/sources.json should be created"
    );

    let sources_content = std::fs::read_to_string(&npins_sources).unwrap();
    assert!(sources_content.contains("\"pins\": {}"));
    assert!(sources_content.contains("\"version\": 7"));

    let npins_default_content = std::fs::read_to_string(&npins_default).unwrap();
    assert!(npins_default_content.contains("mkFunctor"));

    let package_content = std::fs::read_to_string(&package_nix).unwrap();
    assert!(package_content.contains("stdenv.mkDerivation"));

    // Snapshot stable artifacts
    insta::assert_snapshot!("init_npins_wrapper_stdenv", wrapper_content);
    insta::assert_snapshot!("init_npins_overlay_stdenv", overlay_content);
    insta::assert_snapshot!("init_npins_sources_json", sources_content);
    insta::assert_snapshot!("init_npins_default_nix", npins_default_content);
}

/// Test that --init-npins works for python (wrapper should use python3Packages).
///
/// NOTE: This test is disabled because --init-npins is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_init_npins_python_wrapper() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "python_package",
            "-p",
            "requests",
            "-v",
            "2.31.0",
            "-l",
            "asl20",
            "--maintainer",
            "",
            "--init-npins",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    let wrapper = std::fs::read_to_string(temp_path.join("default.nix")).unwrap();
    assert!(
        wrapper.contains("pkgs.python3Packages.requests"),
        "Python wrapper should resolve via python3Packages; got:\n{}",
        wrapper
    );

    // The overlay should use python3Packages.callPackage.
    let overlay = std::fs::read_to_string(temp_path.join("nix").join("overlay.nix")).unwrap();
    assert!(
        overlay.contains(
            "requests = final.python3Packages.callPackage ./pkgs/requests/package.nix { }"
        ),
        "Python overlay should use python3Packages.callPackage; got:\n{}",
        overlay
    );

    insta::assert_snapshot!("init_npins_wrapper_python", wrapper);
    insta::assert_snapshot!("init_npins_overlay_python", overlay);
}

/// Test --init-npins combined with --init-flake: both scaffolds coexist.
///
/// NOTE: This test is disabled because --init-npins is now only for local project
/// initialization and cannot be used with explicit remote package parameters.
#[test]
#[ignore]
fn test_init_npins_with_init_flake() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "stdenv",
            "-p",
            "hello",
            "-v",
            "1.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "--init-npins",
            "--init-flake",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Structured layout: package + overlay under nix/, flake/default at root,
    // npins/ at root.
    assert!(
        temp_path
            .join("nix")
            .join("pkgs")
            .join("hello")
            .join("package.nix")
            .exists(),
        "nix/pkgs/hello/package.nix should exist"
    );
    assert!(
        temp_path.join("nix").join("overlay.nix").exists(),
        "nix/overlay.nix"
    );
    assert!(
        temp_path.join("default.nix").exists(),
        "top-level default.nix"
    );
    assert!(temp_path.join("flake.nix").exists(), "flake.nix");
    assert!(
        temp_path.join("npins").join("default.nix").exists(),
        "npins/default.nix"
    );
    assert!(
        temp_path.join("npins").join("sources.json").exists(),
        "npins/sources.json"
    );

    // Top-level default.nix imports the overlay applied to npins-pinned nixpkgs.
    let wrapper = std::fs::read_to_string(temp_path.join("default.nix")).unwrap();
    assert!(wrapper.contains("overlays = [ (import ./nix/overlay.nix) ];"));

    // Flake exposes the overlay and references the package under nix/pkgs/.
    let flake = std::fs::read_to_string(temp_path.join("flake.nix")).unwrap();
    assert!(
        flake.contains("overlays.default = import ./nix/overlay.nix"),
        "flake should expose overlays.default; got:\n{}",
        flake
    );
}

/// Test that --init-npins refuses to clobber pre-existing scaffold files
/// (specifically files inside the npins/ directory, which the existing
/// package-path collision check does not cover).
#[test]
fn test_init_npins_refuses_overwrite() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Pre-create the npins lockfile reader the tool would otherwise overwrite.
    std::fs::create_dir_all(temp_path.join("npins")).unwrap();
    std::fs::write(
        temp_path.join("npins").join("default.nix"),
        "# pre-existing npins reader\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "stdenv",
            "-p",
            "hello",
            "-v",
            "1.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "--init-npins",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success(), "Command should have failed");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Refusing to overwrite"),
        "stderr should mention refusal; got:\n{}",
        stderr
    );

    // Pre-existing file should be untouched
    let preserved = std::fs::read_to_string(temp_path.join("npins").join("default.nix")).unwrap();
    assert_eq!(preserved, "# pre-existing npins reader\n");
}

#[test]
fn test_npm_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "npm",
            "-p",
            "example",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's an npm package derivation
    assert!(stdout.contains("buildNpmPackage"));
    assert!(stdout.contains("npmDepsHash"));
    assert!(stdout.contains("finalAttrs"));

    // Snapshot the output
    insta::assert_snapshot!("npm_basic_template", stdout);
}

#[test]
fn test_pnpm_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "pnpm",
            "-p",
            "example",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a pnpm package derivation
    assert!(stdout.contains("stdenv.mkDerivation"));
    assert!(stdout.contains("fetchPnpmDeps"));
    assert!(stdout.contains("pnpmConfigHook"));
    assert!(stdout.contains("nodejs"));
    assert!(stdout.contains("pnpm_10"));
    assert!(stdout.contains("finalAttrs"));

    // Snapshot the output
    insta::assert_snapshot!("pnpm_basic_template", stdout);
}

#[test]
fn test_dotnet_template_basic() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().join("default.nix");

    let output = Command::cargo_bin("nix-template")
        .unwrap()
        .args(&[
            "dotnet",
            "--pname",
            "example",
            "--maintainer",
            "me",
            temp_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read the generated file
    let stdout = fs::read_to_string(&temp_path).expect("Failed to read output file");

    // Verify it contains buildDotnetModule with finalAttrs pattern
    assert!(stdout.contains("buildDotnetModule"));
    assert!(stdout.contains("buildDotnetModule (finalAttrs: {"));
    assert!(stdout.contains("projectFile = \"CHANGE\";"));
    assert!(stdout.contains("nugetDeps = ./deps.json;"));
    // pname should be a string literal, only version uses finalAttrs
    assert!(stdout.contains("repo = \"example\";"));
    assert!(stdout.contains("finalAttrs.version"));

    // Verify proper spacing (blank lines between sections)
    assert!(stdout.contains("version = \"0.0.1\";\n\n  src ="));
    assert!(stdout.contains("  };\n\n  projectFile ="));
    assert!(stdout.contains("  nugetDeps = ./deps.json;  # Run `nix-build -A package-name.passthru.fetch-deps` to generate\n\n  meta ="));

    // Snapshot the output
    insta::assert_snapshot!("dotnet_basic_template", stdout);
}

#[test]
fn test_ruby_template_basic() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().join("default.nix");

    let output = Command::cargo_bin("nix-template")
        .unwrap()
        .args(&[
            "ruby",
            "--pname",
            "example",
            "--maintainer",
            "me",
            temp_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read the generated file
    let stdout = fs::read_to_string(&temp_path).expect("Failed to read output file");

    // Verify it contains bundlerApp (NOT finalAttrs pattern)
    assert!(stdout.contains("bundlerApp"));
    assert!(stdout.contains("bundlerApp {"));
    assert!(!stdout.contains("finalAttrs")); // Ruby doesn't use finalAttrs
    assert!(stdout.contains("gemdir = ./.;"));
    assert!(stdout.contains("exes = [ \"example\" ];"));

    // Verify function header
    assert!(stdout.contains("{ lib\n, bundlerApp"));

    // Verify proper spacing (blank lines between sections)
    assert!(stdout.contains("pname = \"example\";\n  gemdir"));

    // Snapshot the output
    insta::assert_snapshot!("ruby_basic_template", stdout);
}

#[test]
fn test_ruby_template_with_dependency_inference() {
    let temp_dir = TempDir::new().unwrap();

    // Create a Gemfile.lock with known gems that have native dependencies
    let gemfile_lock = temp_dir.path().join("Gemfile.lock");
    fs::write(
        &gemfile_lock,
        r#"GEM
  remote: https://rubygems.org/
  specs:
    nokogiri (1.13.10)
      mini_portile2 (~> 2.8.0)
    pg (1.4.5)
    mini_portile2 (2.8.1)

PLATFORMS
  ruby

DEPENDENCIES
  nokogiri
  pg

BUNDLED WITH
   2.3.0
"#,
    )
    .unwrap();

    let output_path = temp_dir.path().join("default.nix");

    let output = Command::cargo_bin("nix-template")
        .unwrap()
        .current_dir(temp_dir.path())
        .args(&[
            "ruby",
            "--pname",
            "example",
            "--maintainer",
            "me",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Read the generated file
    let content = fs::read_to_string(&output_path).expect("Failed to read output file");

    // Without --from-url, dependencies should NOT be inferred
    // (inference only happens with --from-url flag)
    assert!(!content.contains("buildInputs"));
    assert!(!content.contains("nativeBuildInputs"));
    assert!(!content.contains("libxml2"));
    assert!(!content.contains("postgresql"));
}

/// Test that file writes successfully complete and show success messages
/// without crashing due to canonicalize failures.
/// This is a regression test for the post-write canonicalize crash bug.
#[test]
fn test_file_write_shows_success_message() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("test_package.nix");

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "stdenv",
            "-p",
            "test-package",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test User",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Command should succeed
    assert!(
        output.status.success(),
        "Command failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // File should be created
    assert!(
        output_path.exists(),
        "Output file was not created at {:?}",
        output_path
    );

    // Success message should be printed (not crash before printing)
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Generated") && stdout.contains("test_package.nix"),
        "Success message not found in stdout: {}",
        stdout
    );

    // Verify file has valid content
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("stdenv.mkDerivation"));
    assert!(content.contains("test-package"));
}

/// Test that write_new atomically prevents overwriting files.
/// This is a regression test for the TOCTOU race condition bug.
/// The atomic create_new operation should prevent race conditions where
/// a file could be created between the existence check and write.
#[test]
fn test_write_new_atomic_overwrite_prevention() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("existing.nix");

    // Pre-create a file
    fs::write(&output_path, "# original content\n").unwrap();

    // Try to generate a file at the same path
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "stdenv",
            "-p",
            "test-overwrite",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Command should fail
    assert!(
        !output.status.success(),
        "Command should have failed when trying to overwrite existing file"
    );

    // Error message should indicate refusal to overwrite
    // Either from early check in cli.rs or from atomic operation in output.rs
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Refusing to overwrite") || stderr.contains("already exists"),
        "Error should mention file conflict, got: {}",
        stderr
    );

    // Original file should be preserved unchanged
    let preserved_content = fs::read_to_string(&output_path).unwrap();
    assert_eq!(
        preserved_content, "# original content\n",
        "Original file should not have been modified"
    );
}

/// Test that write_new won't follow symlinks to overwrite target files.
/// This prevents symlink attack scenarios.
#[test]
#[cfg(unix)] // Symlinks work differently on Windows
fn test_write_new_prevents_symlink_attacks() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let target_file = temp_dir.path().join("important_file.txt");
    let symlink_path = temp_dir.path().join("test_symlink.nix");

    // Create an important target file
    fs::write(&target_file, "important data\n").unwrap();

    // Create a symlink pointing to the important file
    symlink(&target_file, &symlink_path).unwrap();

    // Try to generate a file at the symlink path
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "stdenv",
            "-p",
            "symlink-test",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test",
            symlink_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Command should fail because symlink already exists
    assert!(
        !output.status.success(),
        "Command should fail when symlink exists at target path"
    );

    // Important: the target file should remain unchanged
    let preserved_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(
        preserved_content, "important data\n",
        "Target file should not have been modified through symlink"
    );
}

/// Test that the program handles corrupted config files gracefully
/// This is a regression test for the XDG/Config unwrap crash bug.
#[test]
fn test_corrupted_config_file_doesnt_crash() {
    let temp_dir = TempDir::new().unwrap();

    // Set XDG_CONFIG_HOME to our temp directory
    let config_dir = temp_dir.path().join("nix-template");
    fs::create_dir_all(&config_dir).unwrap();

    // Create a corrupted (non-UTF8) config file
    let config_file = config_dir.join("config.toml");
    fs::write(&config_file, b"\xFF\xFE invalid utf8 \x80\x81").unwrap();

    // Run nix-template with the corrupted config
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(&[
            "stdenv",
            "-p",
            "test-config",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test",
            "-s", // Use stdout to avoid file creation
        ])
        .output()
        .unwrap();

    // Command should succeed (not crash) despite corrupted config
    assert!(
        output.status.success(),
        "Program should not crash with corrupted config file. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should show warning about config issue
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning") || stderr.contains("Could not read"),
        "Should warn about config issue, got: {}",
        stderr
    );

    // Should still produce valid output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stdenv.mkDerivation"),
        "Should still produce valid output despite config error"
    );
}

/// Test that the program handles malformed TOML config gracefully
#[test]
fn test_malformed_toml_config_doesnt_crash() {
    let temp_dir = TempDir::new().unwrap();

    // Set XDG_CONFIG_HOME to our temp directory
    let config_dir = temp_dir.path().join("nix-template");
    fs::create_dir_all(&config_dir).unwrap();

    // Create a malformed TOML config file
    let config_file = config_dir.join("config.toml");
    fs::write(&config_file, "this is [ not valid = toml {{").unwrap();

    // Run nix-template with the malformed config
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .args(&[
            "stdenv",
            "-p",
            "test-toml",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test",
            "-s", // Use stdout to avoid file creation
        ])
        .output()
        .unwrap();

    // Command should succeed (not crash) despite malformed TOML
    assert!(
        output.status.success(),
        "Program should not crash with malformed TOML config. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should show warning about config parsing issue
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning") || stderr.contains("Could not parse"),
        "Should warn about TOML parsing issue, got: {}",
        stderr
    );

    // Should still produce valid output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stdenv.mkDerivation"),
        "Should still produce valid output despite config error"
    );
}

/// Test that the program works when config directory cannot be created
/// (simulated by using a read-only location)
#[test]
#[cfg(unix)] // Unix-specific test using permissions
fn test_no_config_directory_doesnt_crash() {
    // This test verifies that if XDG setup fails entirely,
    // the program uses fallback and continues

    let temp_dir = TempDir::new().unwrap();
    let readonly_dir = temp_dir.path().join("readonly");
    fs::create_dir(&readonly_dir).unwrap();

    // Make directory read-only
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
    perms.set_mode(0o444); // Read-only
    fs::set_permissions(&readonly_dir, perms).unwrap();

    // Try to use the read-only directory as config home
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .env("XDG_CONFIG_HOME", &readonly_dir)
        .args(&[
            "stdenv",
            "-p",
            "test-readonly",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "Test",
            "-s",
        ])
        .output()
        .unwrap();

    // Program should still succeed (with warning)
    assert!(
        output.status.success(),
        "Program should not crash when config directory is read-only. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should produce valid output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stdenv.mkDerivation"),
        "Should still produce valid output despite XDG issues"
    );

    // Clean up: restore permissions so temp_dir can be deleted
    let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&readonly_dir, perms).unwrap();
}

/// Test basic PHP template generation
#[test]
fn test_php_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "php",
            "-p",
            "laravel",
            "-v",
            "10.0.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s", // --stdout flag
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a PHP Composer derivation
    assert!(stdout.contains("php.buildComposerProject2"));
    assert!(stdout.contains("vendorHash"));

    // Snapshot the output
    insta::assert_snapshot!("php_basic_template", stdout);
}

/// Test PHP template with extensions detected from composer.json
#[test]
fn test_php_template_with_extensions() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a composer.json with PHP extension requirements
    let composer_json = r#"{
    "name": "test/app",
    "require": {
        "php": "^8.3",
        "ext-pdo": "*",
        "ext-mysqli": "*",
        "ext-gd": "*"
    }
}"#;
    fs::write(temp_path.join("composer.json"), composer_json).unwrap();
    fs::write(temp_path.join("composer.lock"), "{}").unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(&temp_path)
        .args(&[
            "php",
            "-p",
            "test-app",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s", // --stdout flag (no --init-flake to avoid temp dir in snapshot)
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify PHP extension wrapper is generated
    // Note: When version is detected from composer.json, it will be php83.buildEnv
    // Otherwise it will be php.buildEnv
    assert!(stdout.contains("php83.buildEnv") || stdout.contains("php.buildEnv"));
    assert!(stdout.contains("extensions ="));
    assert!(stdout.contains("pdo"));
    assert!(stdout.contains("mysqli"));
    assert!(stdout.contains("gd"));
    assert!(stdout.contains("php.buildComposerProject2"));
    assert!(stdout.contains("vendorHash"));

    // Snapshot the entire output
    insta::assert_snapshot!("php_with_extensions_template", stdout);
}

/// Test basic Maven template generation
#[test]
fn test_maven_basic_template() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "maven",
            "-p",
            "spring-boot-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s", // --stdout flag
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Maven derivation
    assert!(stdout.contains("maven.buildMavenPackage"));
    assert!(stdout.contains("mvnHash"));

    // Snapshot the output
    insta::assert_snapshot!("maven_basic_template", stdout);
}

/// Test Maven template with JDBC dependencies detected from pom.xml
#[test]
fn test_maven_template_with_jdbc() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a pom.xml with JDBC dependencies
    let pom_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
         http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
    <artifactId>test-app</artifactId>
    <version>1.0.0</version>

    <properties>
        <maven.compiler.source>21</maven.compiler.source>
        <maven.compiler.target>21</maven.compiler.target>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.postgresql</groupId>
            <artifactId>postgresql</artifactId>
            <version>42.6.0</version>
        </dependency>
    </dependencies>
</project>"#;
    fs::write(temp_path.join("pom.xml"), pom_xml).unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(&temp_path)
        .args(&[
            "maven",
            "-p",
            "test-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s", // --stdout flag
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify Maven derivation basics
    assert!(stdout.contains("maven.buildMavenPackage"));
    assert!(stdout.contains("mvnHash"));

    // Snapshot the entire output (dependencies would be inferred if implemented)
    insta::assert_snapshot!("maven_with_jdbc_template", stdout);
}

/// Test basic Elixir template generation (defaults to Release variant)
#[test]
fn test_elixir_basic_template() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "elixir",
            "-p",
            "phoenix_app",
            "-v",
            "1.0.0",
            "-l",
            "mit",
            "--maintainer",
            "",
            "-s", // --stdout flag
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's an Elixir derivation
    assert!(stdout.contains("beamPackages.mixRelease")); // Default is Release
    assert!(stdout.contains("mixFodDeps"));
    assert!(stdout.contains("beamPackages.fetchMixDeps"));

    // Snapshot the output
    insta::assert_snapshot!("elixir_basic_template", stdout);
}

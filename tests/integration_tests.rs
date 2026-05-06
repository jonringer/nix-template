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
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
            "-s",  // --stdout flag
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
#[test]
fn test_python_template_with_flake_init() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
            "-s",  // --stdout flag
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
    let after_flake_marker: Vec<&str> =
        stdout.split("# ===== flake.nix =====").collect();
    assert_eq!(after_flake_marker.len(), 2, "Expected a flake.nix marker");
    let package_nix = after_flake_marker[0].trim();

    let after_overlay_marker: Vec<&str> =
        after_flake_marker[1].split("# ===== nix/overlay.nix =====").collect();
    assert_eq!(after_overlay_marker.len(), 2, "Expected an overlay marker");
    let flake_nix = after_overlay_marker[0].trim();
    let overlay_nix = after_overlay_marker[1].trim();

    // Verify package part
    assert!(package_nix.contains("buildPythonPackage"));
    assert!(package_nix.contains("fetchPypi"));

    // Overlay must use python3Packages.callPackage for python templates.
    assert!(
        overlay_nix.contains("requests = final.python3Packages.callPackage ./pkgs/requests/package.nix"),
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
            "-f", "pypi",  // Explicitly specify PyPI fetcher
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
            "-s",
            "--init-flake",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify fetchPypi is used
    assert!(stdout.contains("fetchPypi"));
    assert!(!stdout.contains("fetchFromGitHub"), "Should not use GitHub fetcher");

    // Split and snapshot
    let parts: Vec<&str> = stdout.split("# ===== flake.nix =====").collect();
    let package_nix = parts[0].trim();

    insta::assert_snapshot!("python_pypi_explicit_package", package_nix);
}

/// Test Python template with GitHub fetcher override
/// This verifies that we can override the default PyPI fetcher with GitHub.
/// `--init-flake` (no PATH) now produces the structured nix/ layout, so the
/// flake's overlay (not the flake itself) is what wires python3Packages.
#[test]
fn test_python_template_github_fetcher_override() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python_package",
            "-f", "github",  // Override with GitHub fetcher
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
            "-s",
            "--init-flake",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify fetchFromGitHub is used instead of fetchPypi
    assert!(stdout.contains("fetchFromGitHub"));
    assert!(!stdout.contains("fetchPypi"), "Should not use PyPI fetcher");
    assert!(stdout.contains("buildPythonPackage"));

    // Structured layout markers must be present.
    assert!(stdout.contains("# ===== flake.nix ====="));
    assert!(stdout.contains("# ===== nix/overlay.nix ====="));

    // stdout ordering from main.rs is: package → flake → overlay.
    let after_flake_marker: Vec<&str> =
        stdout.split("# ===== flake.nix =====").collect();
    assert_eq!(after_flake_marker.len(), 2, "Expected a flake.nix marker");
    let package_nix = after_flake_marker[0].trim();

    let after_overlay_marker: Vec<&str> =
        after_flake_marker[1].split("# ===== nix/overlay.nix =====").collect();
    assert_eq!(after_overlay_marker.len(), 2, "Expected an overlay marker");
    let flake_nix = after_overlay_marker[0].trim();
    let overlay_nix = after_overlay_marker[1].trim();

    // python3Packages.callPackage now lives in the overlay (not the flake).
    assert!(
        overlay_nix.contains("python3Packages.callPackage"),
        "overlay should wire python3Packages.callPackage; got:\n{}",
        overlay_nix
    );
    assert!(
        flake_nix.contains("overlayed.python3Packages.requests"),
        "flake should resolve via overlayed python3Packages; got:\n{}",
        flake_nix
    );

    insta::assert_snapshot!("python_github_override_package", package_nix);
    insta::assert_snapshot!("python_github_override_overlay", overlay_nix);
    insta::assert_snapshot!("python_github_override_flake", flake_nix);
}

/// Test Python template file writing (not just stdout)
/// `--init-flake` without an explicit PATH now uses the structured nix/
/// layout, so files land at nix/pkgs/<pname>/package.nix, nix/overlay.nix,
/// and flake.nix at the top. No top-level default.nix is emitted in this
/// mode (it's only added by --init-npins / --init-project).
#[test]
fn test_python_template_file_writing_with_flake() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "python_package",
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
            "--init-flake",
            // No -s flag, so it will write files
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Verify the structured layout was created.
    let package_nix_path = temp_path.join("nix/pkgs/requests/package.nix");
    let overlay_nix_path = temp_path.join("nix/overlay.nix");
    let flake_nix_path = temp_path.join("flake.nix");
    let top_default_nix_path = temp_path.join("default.nix");

    assert!(
        package_nix_path.exists(),
        "nix/pkgs/requests/package.nix should be created"
    );
    assert!(overlay_nix_path.exists(), "nix/overlay.nix should be created");
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
#[test]
fn test_init_npins_stdout() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "stdenv",
            "-p", "hello",
            "-v", "1.0",
            "-l", "mit",
            "--maintainer", "",
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
    assert!(stdout.contains("(import sources.nixpkgs { }).extend (import ./nix/overlay.nix)"));
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
#[test]
fn test_init_npins_writes_three_files_and_renames() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "stdenv",
            "-p", "hello",
            "-v", "1.0",
            "-l", "mit",
            "--maintainer", "",
            "--init-npins",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Package lives under nix/pkgs/<pname>/package.nix
    let package_nix = temp_path.join("nix").join("pkgs").join("hello").join("package.nix");
    assert!(package_nix.exists(), "nix/pkgs/hello/package.nix should be created");

    // Top-level default.nix wraps the overlay, replacing the legacy npins
    // wrapper that used to live alongside the package file.
    let wrapper = temp_path.join("default.nix");
    assert!(wrapper.exists(), "top-level default.nix should be created");
    let wrapper_content = std::fs::read_to_string(&wrapper).unwrap();
    assert!(wrapper_content.contains("sources = import ./npins;"));
    assert!(wrapper_content.contains("(import sources.nixpkgs { }).extend (import ./nix/overlay.nix)"));
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
    assert!(npins_default.exists(), "npins/default.nix should be created");
    assert!(npins_sources.exists(), "npins/sources.json should be created");

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
#[test]
fn test_init_npins_python_wrapper() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "python_package",
            "-p", "requests",
            "-v", "2.31.0",
            "-l", "asl20",
            "--maintainer", "",
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
        overlay.contains("requests = final.python3Packages.callPackage ./pkgs/requests/package.nix { }"),
        "Python overlay should use python3Packages.callPackage; got:\n{}",
        overlay
    );

    insta::assert_snapshot!("init_npins_wrapper_python", wrapper);
    insta::assert_snapshot!("init_npins_overlay_python", overlay);
}

/// Test --init-npins combined with --init-flake: both scaffolds coexist.
#[test]
fn test_init_npins_with_init_flake() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "stdenv",
            "-p", "hello",
            "-v", "1.0",
            "-l", "mit",
            "--maintainer", "",
            "--init-npins",
            "--init-flake",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);

    // Structured layout: package + overlay under nix/, flake/default at root,
    // npins/ at root.
    assert!(
        temp_path.join("nix").join("pkgs").join("hello").join("package.nix").exists(),
        "nix/pkgs/hello/package.nix should exist"
    );
    assert!(temp_path.join("nix").join("overlay.nix").exists(), "nix/overlay.nix");
    assert!(temp_path.join("default.nix").exists(), "top-level default.nix");
    assert!(temp_path.join("flake.nix").exists(), "flake.nix");
    assert!(temp_path.join("npins").join("default.nix").exists(), "npins/default.nix");
    assert!(temp_path.join("npins").join("sources.json").exists(), "npins/sources.json");

    // Top-level default.nix imports the overlay applied to npins-pinned nixpkgs.
    let wrapper = std::fs::read_to_string(temp_path.join("default.nix")).unwrap();
    assert!(wrapper.contains("(import sources.nixpkgs { }).extend (import ./nix/overlay.nix)"));

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
            "-p", "hello",
            "-v", "1.0",
            "-l", "mit",
            "--maintainer", "",
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
    let preserved =
        std::fs::read_to_string(temp_path.join("npins").join("default.nix")).unwrap();
    assert_eq!(preserved, "# pre-existing npins reader\n");
}

#[test]
fn test_npm_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "npm",
            "-p", "example",
            "-v", "1.0.0",
            "-l", "mit",
            "--maintainer", "",
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
            "-p", "example",
            "-v", "1.0.0",
            "-l", "mit",
            "--maintainer", "",
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

    let output = Command::cargo_bin("nix-template").unwrap()
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
    assert!(stdout.contains("finalAttrs.pname"));
    assert!(stdout.contains("finalAttrs.version"));

    // Verify proper spacing (blank lines between sections)
    assert!(stdout.contains("version = \"0.0.1\";\n\n  src ="));
    assert!(stdout.contains("  };\n\n  projectFile ="));
    assert!(stdout.contains("  nugetDeps = ./deps.json;  # Run `nix-build -A package-name.passthru.fetch-deps` to generate\n\n  meta ="));

    // Snapshot the output
    insta::assert_snapshot!("dotnet_basic_template", stdout);
}

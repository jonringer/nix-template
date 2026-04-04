use assert_cmd::Command;
use tempfile::TempDir;

/// Test basic Python template generation without --flake-init
/// This should generate only a default.nix file
#[test]
fn test_python_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python",
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

/// Test Python template generation WITH --flake-init
/// This should generate both default.nix and flake.nix to stdout
#[test]
fn test_python_template_with_flake_init() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python",
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

    // Should contain both files separated by the marker
    assert!(stdout.contains("# ===== flake.nix ====="));

    // Split the output into package and flake sections
    let parts: Vec<&str> = stdout.split("# ===== flake.nix =====").collect();
    assert_eq!(parts.len(), 2, "Expected both default.nix and flake.nix output");

    let package_nix = parts[0].trim();
    let flake_nix = parts[1].trim();

    // Verify package part
    assert!(package_nix.contains("buildPythonPackage"));
    assert!(package_nix.contains("fetchPypi"));

    // Verify flake part
    assert!(flake_nix.contains("description ="));
    assert!(flake_nix.contains("inputs"));
    assert!(flake_nix.contains("nixpkgs.url"));
    assert!(flake_nix.contains("outputs"));
    assert!(flake_nix.contains("python3Packages.callPackage"),
            "Python templates should use python3Packages.callPackage");
    assert!(flake_nix.contains("supportedSystems"));

    // Snapshot both parts
    insta::assert_snapshot!("python_with_flake_package", package_nix);
    insta::assert_snapshot!("python_with_flake_flake", flake_nix);
}

/// Test Python template with explicit PyPI fetcher
/// This verifies that -f pypi works correctly
#[test]
fn test_python_template_pypi_fetcher_explicit() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python",
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
/// This verifies that we can override the default PyPI fetcher with GitHub
#[test]
fn test_python_template_github_fetcher_override() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "python",
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

    // Split and snapshot
    let parts: Vec<&str> = stdout.split("# ===== flake.nix =====").collect();
    let package_nix = parts[0].trim();
    let flake_nix = parts[1].trim();

    // Still should use python3Packages in flake
    assert!(flake_nix.contains("python3Packages.callPackage"));

    insta::assert_snapshot!("python_github_override_package", package_nix);
    insta::assert_snapshot!("python_github_override_flake", flake_nix);
}

/// Test Python template file writing (not just stdout)
/// This verifies that files are created correctly in a directory
#[test]
fn test_python_template_file_writing_with_flake() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(temp_path)
        .args(&[
            "python",
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

    // Verify both files were created
    let default_nix_path = temp_path.join("default.nix");
    let flake_nix_path = temp_path.join("flake.nix");

    assert!(default_nix_path.exists(), "default.nix should be created");
    assert!(flake_nix_path.exists(), "flake.nix should be created");

    // Read and verify contents
    let default_nix_content = std::fs::read_to_string(&default_nix_path).unwrap();
    let mut flake_nix_content = std::fs::read_to_string(&flake_nix_path).unwrap();

    assert!(default_nix_content.contains("buildPythonPackage"));
    assert!(flake_nix_content.contains("python3Packages.callPackage"));

    // Normalize the temp directory name in the description field for snapshot stability
    // The description is based on the directory name, which is random for temp dirs
    let temp_dir_name = temp_path.file_name().unwrap().to_str().unwrap();
    flake_nix_content = flake_nix_content.replace(
        &format!("description = \"{}\";", temp_dir_name),
        "description = \"<temp_dir>\";",
    );

    // Snapshot the file contents
    insta::assert_snapshot!("python_file_write_default", default_nix_content);
    insta::assert_snapshot!("python_file_write_flake", flake_nix_content);
}

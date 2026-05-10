use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test basic Python template generation
/// This should generate only a default.nix file
#[test]
fn test_python_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
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

/// Test Python template with explicit PyPI fetcher
/// This verifies that -f pypi works correctly
#[test]
fn test_python_template_pypi_fetcher_explicit() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
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
            "template",
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

/// Test --init-npins refuses to clobber pre-existing scaffold files
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
            "project",
            "npins",
            "stdenv",
            "-p",
            "hello",
            "-v",
            "1.0",
            "-l",
            "mit",
            "--maintainer",
            "",
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
            "template",
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
            "template",
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
            "template",
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
            "template",
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
            "template",
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
#[test]
fn test_file_write_shows_success_message() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("test_package.nix");

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
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
            "template",
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
#[test]
#[cfg(unix)]
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
            "template",
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
            "template",
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
            "template",
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
#[test]
#[cfg(unix)]
fn test_no_config_directory_doesnt_crash() {
    let temp_dir = TempDir::new().unwrap();
    let readonly_dir = temp_dir.path().join("readonly");
    fs::create_dir(&readonly_dir).unwrap();

    // Make directory read-only
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
    perms.set_mode(0o444);
    fs::set_permissions(&readonly_dir, perms).unwrap();

    // Try to use the read-only directory as config home
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .env("XDG_CONFIG_HOME", &readonly_dir)
        .args(&[
            "template",
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
            "template",
            "php",
            "-p",
            "laravel",
            "-v",
            "10.0.0",
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
            "template",
            "php",
            "-p",
            "test-app",
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

    // Verify PHP extension wrapper is generated
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
            "template",
            "maven",
            "-p",
            "spring-boot-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s",
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
            "template",
            "maven",
            "-p",
            "test-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify Maven derivation basics
    assert!(stdout.contains("maven.buildMavenPackage"));
    assert!(stdout.contains("mvnHash"));

    // Snapshot the entire output
    insta::assert_snapshot!("maven_with_jdbc_template", stdout);
}

/// Test basic Elixir template generation (defaults to Release variant)
#[test]
fn test_elixir_basic_template() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "elixir",
            "-p",
            "phoenix_app",
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

    // Verify it's an Elixir derivation
    assert!(stdout.contains("beamPackages.mixRelease"));
    assert!(stdout.contains("mixFodDeps"));
    assert!(stdout.contains("beamPackages.fetchMixDeps"));

    // Snapshot the output
    insta::assert_snapshot!("elixir_basic_template", stdout);
}

/// Test basic Gradle template generation
#[test]
fn test_gradle_basic_template() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a basic build.gradle (Groovy DSL)
    let build_gradle = r#"
plugins {
    id 'java'
}

group = 'com.example'
version = '1.0.0'

java {
    sourceCompatibility = JavaVersion.VERSION_17
}

repositories {
    mavenCentral()
}

dependencies {
    implementation 'com.google.guava:guava:32.1.0-jre'
}
"#;
    fs::write(temp_path.join("build.gradle"), build_gradle).unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(&temp_path)
        .args(&[
            "template",
            "gradle",
            "-p",
            "example-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Gradle derivation with Manual variant
    assert!(stdout.contains("stdenv.mkDerivation"));
    assert!(stdout.contains("gradle.fetchDeps"));
    assert!(stdout.contains("mitmCache"));

    // Snapshot the output
    insta::assert_snapshot!("gradle_basic_template", stdout);
}

/// Test Gradle template with gradle2nix (gradle-deps.json present)
#[test]
fn test_gradle_gradle2nix_template() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create build.gradle.kts (Kotlin DSL)
    let build_gradle_kts = r#"
plugins {
    java
}

group = "com.example"
version = "1.0.0"

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("org.springframework.boot:spring-boot-starter-web:3.2.0")
}
"#;
    fs::write(temp_path.join("build.gradle.kts"), build_gradle_kts).unwrap();

    // Create gradle-deps.json to trigger Gradle2nix variant
    let gradle_deps = r#"{
  "dependencies": []
}"#;
    fs::write(temp_path.join("gradle-deps.json"), gradle_deps).unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(&temp_path)
        .args(&[
            "template",
            "gradle",
            "-p",
            "spring-app",
            "-v",
            "1.0.0",
            "-l",
            "apache20",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Gradle derivation
    assert!(stdout.contains("stdenv.mkDerivation"));
    assert!(stdout.contains("gradle.fetchDeps"));

    // Snapshot the output
    insta::assert_snapshot!("gradle_gradle2nix_template", stdout);
}

/// Test basic Dart template generation with executables
#[test]
fn test_dart_basic_template() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create a pubspec.yaml with executables
    let pubspec_yaml = r#"
name: dart_cli_app
description: A Dart CLI application
version: 1.0.0

environment:
  sdk: '>=3.0.0 <4.0.0'

dependencies:
  args: ^2.4.0
  http: ^0.13.0

executables:
  myapp: main
  helper: helper_tool
"#;
    fs::write(temp_path.join("pubspec.yaml"), pubspec_yaml).unwrap();

    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .current_dir(&temp_path)
        .args(&[
            "template",
            "dart",
            "-p",
            "dart-cli-app",
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

    // Verify it's a Dart derivation
    assert!(stdout.contains("buildDartApplication"));
    assert!(stdout.contains("pubspecLock"));
    assert!(stdout.contains("lib.importJSON ./pubspec.lock.json"));

    // Snapshot the output
    insta::assert_snapshot!("dart_basic_template", stdout);
}

/// Test basic Haskell template generation
#[test]
fn test_haskell_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "haskell",
            "-p",
            "my-haskell-pkg",
            "-v",
            "1.0.0",
            "-l",
            "bsd3",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Haskell package derivation
    assert!(stdout.contains("haskellPackages"));
    assert!(stdout.contains("callCabal2nix"));
    assert!(stdout.contains("# callCabal2nix automatically reads dependencies from the .cabal file"));

    // Snapshot the output
    insta::assert_snapshot!("haskell_basic_template", stdout);
}

/// Test basic OCaml template generation
#[test]
fn test_ocaml_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "ocaml",
            "-p",
            "my-ocaml-pkg",
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

    // Verify it's an OCaml package derivation
    assert!(stdout.contains("buildDunePackage"));
    assert!(stdout.contains("# buildDunePackage reads dependencies from dune-project"));
    assert!(stdout.contains("opam-nix"));

    // Snapshot the output
    insta::assert_snapshot!("ocaml_basic_template", stdout);
}

/// Test basic Scala template generation
#[test]
fn test_scala_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "scala",
            "-p",
            "my-scala-app",
            "-v",
            "1.0.0",
            "-l",
            "apache2",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Scala/SBT derivation
    assert!(stdout.contains("stdenv.mkDerivation"));
    assert!(stdout.contains("# SBT dependencies are fetched using a Fixed Output Derivation"));
    assert!(stdout.contains("sbt-derivation"));

    // Snapshot the output
    insta::assert_snapshot!("scala_basic_template", stdout);
}

#[test]
fn test_clojure_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "clojure",
            "-p",
            "my-clojure-app",
            "-v",
            "1.0.0",
            "-l",
            "epl10",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Clojure derivation
    assert!(stdout.contains("stdenv.mkDerivation"));
    assert!(stdout.contains("# Clojure dependencies are managed via clj-nix"));
    assert!(stdout.contains("clj-nix"));

    // Snapshot the output
    insta::assert_snapshot!("clojure_basic_template", stdout);
}

/// Test basic Perl template generation with buildPerlPackage (MakeMaker)
#[test]
fn test_perl_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "perl",
            "-p",
            "My-Module",
            "-v",
            "1.0.0",
            "-l",
            "artistic2",
            "--maintainer",
            "",
            "-s",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify it's a Perl derivation
    assert!(stdout.contains("buildPerlPackage"));
    assert!(stdout.contains("# Perl dependencies are typically handled automatically"));

    // Snapshot the output
    insta::assert_snapshot!("perl_basic_template", stdout);
}

/// Test basic Lua template generation with buildLuaPackage
#[test]
fn test_lua_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "lua",
            "-p",
            "my-lua-lib",
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

    // Verify it's a Lua derivation
    assert!(stdout.contains("buildLuaPackage"));
    assert!(stdout.contains("# Lua dependencies from .rockspec are handled automatically"));

    // Snapshot the output
    insta::assert_snapshot!("lua_basic_template", stdout);
}

/// Test basic R template generation with rPackages.buildRPackage
#[test]
fn test_r_template_basic() {
    let mut cmd = Command::cargo_bin("nix-template").unwrap();
    let output = cmd
        .args(&[
            "template",
            "r",
            "-p",
            "myRpackage",
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

    // Verify it's an R derivation
    assert!(stdout.contains("rPackages.buildRPackage"));
    assert!(stdout.contains("# R dependencies from DESCRIPTION are handled automatically"));

    // Snapshot the output
    insta::assert_snapshot!("r_basic_template", stdout);
}

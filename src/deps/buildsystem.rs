//! Inference of build system → nixpkgs build inputs.
//!
//! This module detects build systems (CMake, Meson) and parses their
//! dependency declarations to map to nixpkgs packages.
//!
//! The approach is conservative: only high-confidence, simple patterns
//! are detected. Complex conditionals and variables are ignored.

use crate::types::ExpressionInfo;
use log::debug;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const LOG_TARGET: &str = "nix-template::buildsystem";

/// Static mapping from CMake package name to nixpkgs equivalent.
///
/// Each tuple is `(build_inputs, native_build_inputs)`.
/// Only includes well-known, high-confidence mappings.
fn lookup_cmake_package(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match name {
        // Core libraries
        "ZLIB" => Some((&["zlib"], &[])),
        "PNG" => Some((&["libpng"], &[])),
        "JPEG" => Some((&["libjpeg"], &[])),
        "TIFF" => Some((&["libtiff"], &[])),
        "GIF" => Some((&["giflib"], &[])),

        // Compression
        "BZip2" => Some((&["bzip2"], &[])),
        "LibLZMA" | "LZMA" => Some((&["xz"], &[])),
        "ZSTD" => Some((&["zstd"], &[])),

        // Crypto & Security
        "OpenSSL" => Some((&["openssl"], &["pkg-config"])),
        "GnuTLS" => Some((&["gnutls"], &["pkg-config"])),

        // Databases
        "SQLite3" => Some((&["sqlite"], &[])),
        "PostgreSQL" => Some((&["postgresql"], &[])),
        "MySQL" => Some((&["libmysqlclient"], &[])),

        // XML/Text processing
        "LibXml2" => Some((&["libxml2"], &["pkg-config"])),
        "LibXslt" => Some((&["libxslt"], &["pkg-config"])),
        "EXPAT" => Some((&["expat"], &[])),

        // Networking
        "CURL" => Some((&["curl"], &["pkg-config"])),

        // UI Frameworks
        "Qt5" | "Qt5Core" | "Qt5Widgets" => Some((&["qt5.qtbase"], &[])),
        "Qt6" | "Qt6Core" | "Qt6Widgets" => Some((&["qt6.qtbase"], &[])),
        "GTK3" => Some((&["gtk3"], &["pkg-config"])),
        "GTK4" => Some((&["gtk4"], &["pkg-config"])),

        // Other common
        "Boost" => Some((&["boost"], &[])),
        "Protobuf" => Some((&["protobuf"], &["pkg-config"])),
        "GTest" => Some((&["gtest"], &[])),
        "Threads" => Some((&[], &[])), // Built-in, no package needed
        "PkgConfig" => Some((&[], &["pkg-config"])),
        "Python" | "Python3" => Some((&["python3"], &[])),

        // Audio/Video
        "ALSA" => Some((&["alsa-lib"], &["pkg-config"])),
        "PulseAudio" => Some((&["libpulseaudio"], &["pkg-config"])),
        "JACK" => Some((&["libjack2"], &["pkg-config"])),
        "FFmpeg" => Some((&["ffmpeg"], &["pkg-config"])),

        // Graphics
        "OpenGL" => Some((&["libGL"], &[])),
        "Vulkan" => Some((&["vulkan-loader"], &[])),

        _ => None,
    }
}

/// Static mapping from Meson dependency name to nixpkgs equivalent.
///
/// Meson uses pkg-config names, so these map more directly.
fn lookup_meson_dependency(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match name {
        // Core libraries (pkg-config names)
        "zlib" => Some((&["zlib"], &[])),
        "libpng" => Some((&["libpng"], &[])),
        "libjpeg" => Some((&["libjpeg"], &[])),

        // Compression
        "bzip2" => Some((&["bzip2"], &[])),
        "liblzma" => Some((&["xz"], &[])),
        "libzstd" => Some((&["zstd"], &[])),

        // Crypto
        "openssl" => Some((&["openssl"], &[])),
        "gnutls" => Some((&["gnutls"], &[])),

        // Databases
        "sqlite3" => Some((&["sqlite"], &[])),
        "libpq" => Some((&["postgresql"], &[])),

        // XML
        "libxml-2.0" => Some((&["libxml2"], &[])),
        "libxslt" => Some((&["libxslt"], &[])),

        // Networking
        "libcurl" => Some((&["curl"], &[])),

        // UI - GTK
        "gtk+-3.0" => Some((&["gtk3"], &[])),
        "gtk4" => Some((&["gtk4"], &[])),
        "glib-2.0" => Some((&["glib"], &[])),
        "gobject-2.0" => Some((&["glib"], &[])),

        // UI - Qt (note: Meson uses modules)
        "qt5" => Some((&["qt5.qtbase"], &[])),
        "qt6" => Some((&["qt6.qtbase"], &[])),

        // Audio
        "alsa" => Some((&["alsa-lib"], &[])),
        "libpulse" => Some((&["libpulseaudio"], &[])),
        "jack" => Some((&["libjack2"], &[])),

        // Other
        "protobuf" => Some((&["protobuf"], &[])),
        "dbus-1" => Some((&["dbus"], &[])),

        _ => None,
    }
}

/// Parse CMakeLists.txt for find_package() and find_dependency() calls.
///
/// Returns a list of package names found.
/// Only extracts simple, unconditional calls - ignores conditionals and variables.
pub fn parse_cmake_dependencies(cmake_content: &str) -> Vec<String> {
    let mut packages = BTreeSet::new();

    // Regex for find_package(PackageName ...) or find_dependency(PackageName ...)
    // Captures simple cases, ignores variables like ${VAR}
    let re = Regex::new(r"(?m)^\s*find_(?:package|dependency)\s*\(\s*([A-Za-z0-9_]+)")
        .expect("valid regex");

    for cap in re.captures_iter(cmake_content) {
        if let Some(pkg_name) = cap.get(1) {
            let name = pkg_name.as_str();
            // Skip if it looks like a variable
            if !name.starts_with('$') && !name.contains('{') {
                debug!(target: LOG_TARGET, "Found CMake package: {}", name);
                packages.insert(name.to_owned());
            }
        }
    }

    packages.into_iter().collect()
}

/// Parse meson.build for dependency() calls.
///
/// Returns a list of dependency names found.
pub fn parse_meson_dependencies(meson_content: &str) -> Vec<String> {
    let mut deps = BTreeSet::new();

    // Regex for dependency('name') or dependency("name")
    let re = Regex::new(r#"dependency\s*\(\s*['"]([a-zA-Z0-9_+\-\.]+)['"]"#)
        .expect("valid regex");

    for cap in re.captures_iter(meson_content) {
        if let Some(dep_name) = cap.get(1) {
            let name = dep_name.as_str();
            debug!(target: LOG_TARGET, "Found Meson dependency: {}", name);
            deps.insert(name.to_owned());
        }
    }

    deps.into_iter().collect()
}

/// Check if content contains PKG_CHECK_MODULES (autotools pattern).
///
/// If found, we should add pkg-config to nativeBuildInputs.
pub fn has_pkg_check_modules(content: &str) -> bool {
    content.contains("PKG_CHECK_MODULES")
}

/// Detect build system and infer dependencies for stdenv packages.
///
/// This function:
/// 1. Detects presence of CMakeLists.txt or meson.build
/// 2. Auto-adds appropriate build tools (cmake, meson+ninja)
/// 3. Parses dependency declarations
/// 4. Maps to nixpkgs equivalents
///
/// Returns true if a build system was detected.
/// Core inference logic that works with any source path.
/// Returns (build_inputs, native_build_inputs) if any build system files are detected.
fn infer_from_source_path(source_path: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let cmake_file = source_path.join("CMakeLists.txt");
    let meson_file = source_path.join("meson.build");
    let configure_ac = source_path.join("configure.ac");

    let mut detected = false;
    let mut build_inputs: BTreeSet<String> = BTreeSet::new();
    let mut native_build_inputs: BTreeSet<String> = BTreeSet::new();

    // CMake detection
    if cmake_file.exists() {
        debug!(target: LOG_TARGET, "Detected CMakeLists.txt");
        detected = true;

        // Always add cmake to nativeBuildInputs
        native_build_inputs.insert("cmake".to_owned());

        // Parse CMakeLists.txt for dependencies
        if let Ok(content) = std::fs::read_to_string(&cmake_file) {
            let packages = parse_cmake_dependencies(&content);
            debug!(target: LOG_TARGET, "Found {} CMake packages", packages.len());

            for pkg in packages {
                if let Some((bi, nbi)) = lookup_cmake_package(&pkg) {
                    build_inputs.extend(bi.iter().map(|s| s.to_string()));
                    native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
                }
            }
        }
    }

    // Meson detection
    if meson_file.exists() {
        debug!(target: LOG_TARGET, "Detected meson.build");
        detected = true;

        // Always add meson and ninja to nativeBuildInputs
        native_build_inputs.insert("meson".to_owned());
        native_build_inputs.insert("ninja".to_owned());

        // Parse meson.build for dependencies
        if let Ok(content) = std::fs::read_to_string(&meson_file) {
            let deps = parse_meson_dependencies(&content);
            debug!(target: LOG_TARGET, "Found {} Meson dependencies", deps.len());

            // Meson uses pkg-config heavily, so if any dependencies found, add pkg-config
            if !deps.is_empty() {
                native_build_inputs.insert("pkg-config".to_owned());
            }

            for dep in deps {
                if let Some((bi, nbi)) = lookup_meson_dependency(&dep) {
                    build_inputs.extend(bi.iter().map(|s| s.to_string()));
                    native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
                }
            }
        }
    }

    // Autotools detection (configure.ac) - just add pkg-config if PKG_CHECK_MODULES found
    if configure_ac.exists() {
        if let Ok(content) = std::fs::read_to_string(&configure_ac) {
            if has_pkg_check_modules(&content) {
                debug!(target: LOG_TARGET, "Detected PKG_CHECK_MODULES in configure.ac");
                native_build_inputs.insert("pkg-config".to_owned());
                detected = true;
            }
        }
    }

    if detected {
        Some((
            build_inputs.into_iter().collect(),
            native_build_inputs.into_iter().collect(),
        ))
    } else {
        None
    }
}

/// Infer build system dependencies from a local source path.
/// Used during local project initialization (--init-flake/--init-npins).
pub fn infer_buildsystem_dependencies_from_path(
    source_path: &Path,
) -> Option<(Vec<String>, Vec<String>)> {
    eprintln!("Scanning for build system files (CMakeLists.txt, meson.build, configure.ac)...");
    infer_from_source_path(source_path)
}

/// Infer build system dependencies from an already-materialized source in ExpressionInfo.
/// This is the original function used when inferring from remote sources.
pub fn infer_buildsystem_dependencies(info: &mut ExpressionInfo) -> bool {
    if let Some((build_inputs, native_build_inputs)) =
        infer_from_source_path(&info.top_level_path)
    {
        info.build_inputs.extend(build_inputs);
        info.native_build_inputs.extend(native_build_inputs);

        debug!(
            target: LOG_TARGET,
            "Build system inference complete: buildInputs={:?}, nativeBuildInputs={:?}",
            info.build_inputs,
            info.native_build_inputs
        );

        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cmake_find_package() {
        let cmake = r#"
cmake_minimum_required(VERSION 3.10)
project(MyProject)

find_package(ZLIB REQUIRED)
find_package(OpenSSL)
find_package(Qt5 COMPONENTS Core Widgets)
        "#;

        let packages = parse_cmake_dependencies(cmake);
        assert!(packages.contains(&"ZLIB".to_string()));
        assert!(packages.contains(&"OpenSSL".to_string()));
        assert!(packages.contains(&"Qt5".to_string()));
    }

    #[test]
    fn parse_cmake_find_dependency() {
        let cmake = r#"
find_dependency(Boost REQUIRED)
find_dependency(Protobuf)
        "#;

        let packages = parse_cmake_dependencies(cmake);
        assert!(packages.contains(&"Boost".to_string()));
        assert!(packages.contains(&"Protobuf".to_string()));
    }

    #[test]
    fn parse_cmake_ignores_variables() {
        let cmake = r#"
find_package(${MY_VAR} REQUIRED)
find_package(ValidPackage)
        "#;

        let packages = parse_cmake_dependencies(cmake);
        assert!(!packages.contains(&"${MY_VAR}".to_string()));
        assert!(packages.contains(&"ValidPackage".to_string()));
    }

    #[test]
    fn parse_meson_deps() {
        let meson = r#"
project('myproject', 'c')

zlib_dep = dependency('zlib')
openssl = dependency("openssl", required: true)
gtk = dependency('gtk+-3.0', version: '>= 3.20')
        "#;

        let deps = parse_meson_dependencies(meson);
        assert!(deps.contains(&"zlib".to_string()));
        assert!(deps.contains(&"openssl".to_string()));
        assert!(deps.contains(&"gtk+-3.0".to_string()));
    }

    #[test]
    fn lookup_common_cmake_packages() {
        assert!(lookup_cmake_package("ZLIB").is_some());
        assert!(lookup_cmake_package("OpenSSL").is_some());
        assert!(lookup_cmake_package("Qt5").is_some());
        assert!(lookup_cmake_package("UnknownPackage").is_none());

        let (bi, nbi) = lookup_cmake_package("OpenSSL").unwrap();
        assert_eq!(bi, &["openssl"]);
        assert_eq!(nbi, &["pkg-config"]);
    }

    #[test]
    fn lookup_common_meson_deps() {
        assert!(lookup_meson_dependency("zlib").is_some());
        assert!(lookup_meson_dependency("gtk+-3.0").is_some());
        assert!(lookup_meson_dependency("unknown-dep").is_none());

        let (bi, _) = lookup_meson_dependency("zlib").unwrap();
        assert_eq!(bi, &["zlib"]);
    }

    #[test]
    fn detect_pkg_check_modules() {
        let configure = r#"
AC_INIT([myproject], [1.0])
PKG_CHECK_MODULES([GLIB], [glib-2.0 >= 2.40])
        "#;

        assert!(has_pkg_check_modules(configure));
        assert!(!has_pkg_check_modules("no pkg check here"));
    }

    #[test]
    fn infer_from_cmake_project() {
        use crate::types::{ExpressionInfo, Fetcher, Template};
        use std::path::PathBuf;

        let temp_dir = std::env::temp_dir().join("nix-template-cmake-test");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let cmake_file = temp_dir.join("CMakeLists.txt");
        std::fs::write(
            &cmake_file,
            r#"
cmake_minimum_required(VERSION 3.10)
find_package(ZLIB REQUIRED)
find_package(OpenSSL)
            "#,
        )
        .unwrap();

        let mut info = ExpressionInfo {
            pname: "test".to_owned(),
            version: "1.0.0".to_owned(),
            license: "mit".to_owned(),
            maintainer: "me".to_owned(),
            fetcher: Fetcher::github,
            template: Template::stdenv,
            path_to_write: PathBuf::new(),
            top_level_path: temp_dir.clone(),
            include_documentation_links: false,
            include_meta: true,
            tag_prefix: "".to_owned(),
            owner: "test".to_owned(),
            src_sha: "sha256-test".to_owned(),
            description: "test".to_owned(),
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
            use_cargo_lock_file: false,
            cargo_lock_git_deps: Vec::new(),
            python_format: "setuptools".to_owned(),
        };

        let detected = infer_buildsystem_dependencies(&mut info);
        assert!(detected);

        // Should have cmake in nativeBuildInputs
        assert!(info.native_build_inputs.contains(&"cmake".to_string()));

        // Should have inferred zlib and openssl
        assert!(info.build_inputs.contains(&"zlib".to_string()));
        assert!(info.build_inputs.contains(&"openssl".to_string()));

        // OpenSSL requires pkg-config
        assert!(info.native_build_inputs.contains(&"pkg-config".to_string()));

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}

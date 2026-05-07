//! Inference of PHP extensions and native dependencies from `composer.json`.
//!
//! This module reads a PHP project's `composer.json` and extracts:
//! 1. PHP extensions (ext-*) required by the project
//! 2. Native library dependencies for Composer packages
//!
//! PHP extensions are mapped to nixpkgs PHP extension attribute names.
//! Packages requiring native libraries (e.g., imagemagick, mongodb) are
//! identified via a static mapping table.
//!
//! As with other dependency modules, this is best-effort: users can edit
//! the generated expression to add anything we missed.

use log::debug;
use std::collections::BTreeSet;
use std::path::Path;
use serde_json::Value;

const LOG_TARGET: &str = "nix-template::php_deps";

/// Normalise a PHP extension name from composer.json ext-* format.
///
/// Extension names in composer.json are prefixed with "ext-" (e.g., "ext-pdo").
/// This function strips the prefix and returns the extension name in lowercase.
fn normalise_extension_name(ext_name: &str) -> String {
    ext_name
        .strip_prefix("ext-")
        .unwrap_or(ext_name)
        .to_lowercase()
        .replace('_', "")
        .replace('-', "")
}

/// Map PHP extension names to their nixpkgs attribute names.
///
/// Most extensions have the same name, but some have variations or
/// special handling in nixpkgs.
fn lookup_extension(normalised: &str) -> Option<&'static str> {
    match normalised {
        // Common extensions (most map directly)
        "pdo" => Some("pdo"),
        "pdomysql" => Some("pdo_mysql"),
        "pdopgsql" => Some("pdo_pgsql"),
        "pdosqlite" => Some("pdo_sqlite"),
        "mysqli" => Some("mysqli"),
        "gd" => Some("gd"),
        "curl" => Some("curl"),
        "mbstring" => Some("mbstring"),
        "xml" => Some("xml"),
        "zip" => Some("zip"),
        "bcmath" => Some("bcmath"),
        "intl" => Some("intl"),
        "soap" => Some("soap"),
        "sockets" => Some("sockets"),
        "openssl" => Some("openssl"),
        "json" => Some("json"),
        "fileinfo" => Some("fileinfo"),
        "tokenizer" => Some("tokenizer"),
        "ctype" => Some("ctype"),
        "iconv" => Some("iconv"),
        "dom" => Some("dom"),
        "simplexml" => Some("simplexml"),
        "xmlreader" => Some("xmlreader"),
        "xmlwriter" => Some("xmlwriter"),
        "filter" => Some("filter"),
        "hash" => Some("hash"),
        "session" => Some("session"),
        "pcre" => Some("pcre"),
        "spl" => Some("spl"),
        "reflection" => Some("reflection"),
        _ => None, // Unknown extension - user will need to add manually
    }
}

/// Map Composer package names to their native library dependencies.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
/// Most PHP packages don't require native libs, but some do.
fn lookup_composer_package(package_name: &str) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    match package_name {
        // Packages requiring native libraries
        "ext-imagick" | "imagick/imagick" => {
            Some((vec!["imagemagick"], vec![]))
        }
        "mongodb/mongodb" => {
            Some((vec!["mongodb"], vec![]))
        }
        "ext-redis" | "predis/predis" => {
            // Redis PHP extension or client - may need redis server for tests
            None // Usually doesn't need native deps in build
        }
        "guzzlehttp/guzzle" | "symfony/http-client" => {
            // HTTP clients - use ext-curl
            None
        }
        _ => None,
    }
}

/// Parse `composer.json` and extract PHP extensions from the `require` section.
///
/// Returns a set of extension names (normalized, without "ext-" prefix).
pub fn detect_php_extensions(composer_json_path: &Path) -> Vec<String> {
    let contents = match std::fs::read_to_string(composer_json_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read composer.json: {}", e
            );
            return Vec::new();
        }
    };

    let parsed: Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to parse composer.json: {}", e
            );
            return Vec::new();
        }
    };

    let mut extensions = BTreeSet::new();

    // Check require section for ext-* dependencies
    if let Some(require) = parsed.get("require").and_then(|v| v.as_object()) {
        for (name, _version) in require {
            if name.starts_with("ext-") {
                let normalised = normalise_extension_name(name);
                if let Some(ext_name) = lookup_extension(&normalised) {
                    extensions.insert(ext_name.to_string());
                    debug!(
                        target: LOG_TARGET,
                        "detected PHP extension: {} (from {})", ext_name, name
                    );
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "unknown PHP extension: {} (from {})", normalised, name
                    );
                }
            }
        }
    }

    // Check require-dev section as well (extensions needed for tests)
    if let Some(require_dev) = parsed.get("require-dev").and_then(|v| v.as_object()) {
        for (name, _version) in require_dev {
            if name.starts_with("ext-") {
                let normalised = normalise_extension_name(name);
                if let Some(ext_name) = lookup_extension(&normalised) {
                    extensions.insert(ext_name.to_string());
                    debug!(
                        target: LOG_TARGET,
                        "detected PHP extension (dev): {} (from {})", ext_name, name
                    );
                }
            }
        }
    }

    extensions.into_iter().collect()
}

/// Infer native library dependencies from composer.json packages.
///
/// Returns a tuple of (buildInputs, nativeBuildInputs).
pub fn infer_native_dependencies(composer_json_path: &Path) -> (Vec<String>, Vec<String>) {
    let contents = match std::fs::read_to_string(composer_json_path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to read composer.json: {}", e
            );
            return (Vec::new(), Vec::new());
        }
    };

    let parsed: Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            debug!(
                target: LOG_TARGET,
                "failed to parse composer.json: {}", e
            );
            return (Vec::new(), Vec::new());
        }
    };

    let mut build_inputs = BTreeSet::new();
    let mut native_build_inputs = BTreeSet::new();

    // Check require section for packages with native deps
    if let Some(require) = parsed.get("require").and_then(|v| v.as_object()) {
        for (name, _version) in require {
            if let Some((bi, nbi)) = lookup_composer_package(name) {
                build_inputs.extend(bi.iter().map(|s| s.to_string()));
                native_build_inputs.extend(nbi.iter().map(|s| s.to_string()));
                debug!(
                    target: LOG_TARGET,
                    "detected native deps for {}: buildInputs={:?}, nativeBuildInputs={:?}",
                    name, bi, nbi
                );
            }
        }
    }

    (
        build_inputs.into_iter().collect(),
        native_build_inputs.into_iter().collect(),
    )
}

/// Detect the PHP version requirement from composer.json.
///
/// Returns the preferred PHP version as a string (e.g., "83", "82", "81")
/// or None if no specific version is required.
pub fn detect_php_version(composer_json_path: &Path) -> Option<String> {
    let contents = match std::fs::read_to_string(composer_json_path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let parsed: Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Check require.php for version constraint
    if let Some(php_version) = parsed
        .get("require")
        .and_then(|v| v.get("php"))
        .and_then(|v| v.as_str())
    {
        // Parse version like "^8.3", ">=8.2", "~8.1.0"
        // Extract the major.minor version
        let version_str = php_version.trim_start_matches(['^', '~', '>', '<', '=', ' ']);

        if let Some(major_minor) = version_str.split('.').take(2).collect::<Vec<_>>().get(0..2) {
            // Convert "8.3" -> "83", "8.2" -> "82"
            let version = format!("{}{}", major_minor[0], major_minor[1]);
            debug!(
                target: LOG_TARGET,
                "detected PHP version requirement: {} (from {})", version, php_version
            );
            return Some(version);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalise_extension_name() {
        assert_eq!(normalise_extension_name("ext-pdo"), "pdo");
        assert_eq!(normalise_extension_name("ext-pdo_mysql"), "pdomysql");
        assert_eq!(normalise_extension_name("ext-gd"), "gd");
        assert_eq!(normalise_extension_name("pdo"), "pdo");
    }

    #[test]
    fn test_lookup_extension() {
        assert_eq!(lookup_extension("pdo"), Some("pdo"));
        assert_eq!(lookup_extension("pdomysql"), Some("pdo_mysql"));
        assert_eq!(lookup_extension("gd"), Some("gd"));
        assert_eq!(lookup_extension("mbstring"), Some("mbstring"));
        assert_eq!(lookup_extension("unknown"), None);
    }

    #[test]
    fn test_lookup_composer_package() {
        let result = lookup_composer_package("mongodb/mongodb");
        assert!(result.is_some());
        let (bi, _nbi) = result.unwrap();
        assert!(bi.contains(&"mongodb"));
    }
}

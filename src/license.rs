use anyhow::Result;
use askalono::{ScanStrategy, Store, TextData};
use log::debug;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Infer license from a materialized source directory by scanning LICENSE files
pub fn infer_license_from_source(source_path: &Path) -> Result<Option<String>> {
    debug!("Scanning for license files in {:?}", source_path);

    // Create the license store (embedded SPDX database)
    let store = Store::new();
    let mut license_scores: BTreeMap<String, f32> = BTreeMap::new();

    // Search for license files in root directory
    scan_directory_for_licenses(source_path, &store, &mut license_scores)?;

    // Search for REUSE-compliant LICENSES/ directory
    let licenses_dir = source_path.join("LICENSES");
    if licenses_dir.is_dir() {
        debug!("Found REUSE-compliant LICENSES/ directory");
        scan_directory_for_licenses(&licenses_dir, &store, &mut license_scores)?;
    }

    // Convert to nixpkgs license names and format output
    format_license_output(&license_scores)
}

/// Scan a directory for license files and update the scores map
fn scan_directory_for_licenses(
    dir: &Path,
    store: &Store,
    license_scores: &mut BTreeMap<String, f32>,
) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            debug!("Failed to read directory {:?}: {}", dir, e);
            return Ok(());
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();

        // Only process files, not directories
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name.to_lowercase(),
            None => continue,
        };

        // Check if filename matches license patterns
        if is_license_file(&filename) {
            debug!("Scanning license file: {:?}", path);
            if let Err(e) = scan_license_file(&path, store, license_scores) {
                debug!("Failed to scan {:?}: {}", path, e);
            }
        }
    }

    Ok(())
}

/// Check if a filename matches common license file patterns
fn is_license_file(filename: &str) -> bool {
    filename.starts_with("license")
        || filename.starts_with("licence")
        || filename.starts_with("copying")
}

/// Scan a single license file and update the scores map
fn scan_license_file(
    path: &Path,
    store: &Store,
    license_scores: &mut BTreeMap<String, f32>,
) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let strategy = ScanStrategy::new(store).confidence_threshold(0.8);
    let text_data = TextData::from(text.as_str());

    match strategy.scan(&text_data) {
        Ok(result) => {
            // Extract license name from the result
            let license_name = if let Some(identified) = &result.license {
                identified.name.to_string()
            } else {
                debug!("No license identified in {:?}", path);
                return Ok(());
            };
            let score = result.score;

            debug!(
                "Detected license '{}' with confidence {:.2} in {:?}",
                license_name, score, path
            );

            // Keep the highest score for each license
            license_scores
                .entry(license_name)
                .and_modify(|s| *s = s.max(score))
                .or_insert(score);
        }
        Err(e) => {
            debug!("Failed to scan {:?}: {}", path, e);
        }
    }

    Ok(())
}

/// Format detected licenses as nixpkgs license expression
fn format_license_output(license_scores: &BTreeMap<String, f32>) -> Result<Option<String>> {
    if license_scores.is_empty() {
        return Ok(None);
    }

    // Convert SPDX identifiers to nixpkgs license names
    let mut nix_licenses: Vec<String> = license_scores
        .keys()
        .filter_map(|spdx| get_nix_license(spdx))
        .map(String::from)
        .collect();

    if nix_licenses.is_empty() {
        return Ok(None);
    }

    // Sort for deterministic output
    nix_licenses.sort();
    nix_licenses.dedup();

    let output = if nix_licenses.len() == 1 {
        // Single license: lib.licenses.mit
        format!("lib.licenses.{}", nix_licenses[0])
    } else {
        // Multiple licenses: with lib.licenses; [ mit asl20 ]
        format!("with lib.licenses; [ {} ]", nix_licenses.join(" "))
    };

    Ok(Some(output))
}

/// Map SPDX license identifier to nixpkgs license name
/// Based on https://github.com/NixOS/nixpkgs/blob/master/lib/licenses.nix
fn get_nix_license(spdx: &str) -> Option<&'static str> {
    match spdx {
        "0BSD" => Some("bsd0"),
        "AFL-3.0" => Some("afl3"),
        "AGPL-3.0-only" => Some("agpl3Only"),
        "AGPL-3.0-or-later" => Some("agpl3Plus"),
        "Apache-2.0" => Some("asl20"),
        "Artistic-2.0" => Some("artistic2"),
        "BSD-2-Clause" => Some("bsd2"),
        "BSD-3-Clause" => Some("bsd3"),
        "BSL-1.0" => Some("boost"),
        "CC0-1.0" => Some("cc0"),
        "CC-BY-3.0" => Some("cc-by-30"),
        "CC-BY-4.0" => Some("cc-by-40"),
        "CC-BY-SA-3.0" => Some("cc-by-sa-30"),
        "CC-BY-SA-4.0" => Some("cc-by-sa-40"),
        "EPL-1.0" => Some("epl10"),
        "EPL-2.0" => Some("epl20"),
        "EUPL-1.2" => Some("eupl12"),
        "GPL-2.0-only" => Some("gpl2Only"),
        "GPL-2.0-or-later" => Some("gpl2Plus"),
        "GPL-3.0-only" => Some("gpl3Only"),
        "GPL-3.0-or-later" => Some("gpl3Plus"),
        "ISC" => Some("isc"),
        "LGPL-2.1-only" => Some("lgpl21Only"),
        "LGPL-2.1-or-later" => Some("lgpl21Plus"),
        "LGPL-3.0-only" => Some("lgpl3Only"),
        "LGPL-3.0-or-later" => Some("lgpl3Plus"),
        "MIT" => Some("mit"),
        "MPL-2.0" => Some("mpl20"),
        "MS-PL" => Some("mspl"),
        "NCSA" => Some("ncsa"),
        "OFL-1.1" => Some("ofl"),
        "OSL-3.0" => Some("osl3"),
        "PostgreSQL" => Some("postgresql"),
        "Python-2.0" => Some("psfl"),
        "Unlicense" => Some("unlicense"),
        "Zlib" => Some("zlib"),
        _ => {
            debug!("Unknown SPDX license identifier: {}", spdx);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_license_file() {
        assert!(is_license_file("license"));
        assert!(is_license_file("license.txt"));
        assert!(is_license_file("license.md"));
        assert!(is_license_file("licence"));
        assert!(is_license_file("copying"));
        assert!(is_license_file("copying.txt"));
        assert!(!is_license_file("readme"));
        assert!(!is_license_file("src.rs"));
    }

    #[test]
    fn test_get_nix_license() {
        assert_eq!(get_nix_license("MIT"), Some("mit"));
        assert_eq!(get_nix_license("Apache-2.0"), Some("asl20"));
        assert_eq!(get_nix_license("GPL-3.0-only"), Some("gpl3Only"));
        assert_eq!(get_nix_license("GPL-3.0-or-later"), Some("gpl3Plus"));
        assert_eq!(get_nix_license("BSD-3-Clause"), Some("bsd3"));
        assert_eq!(get_nix_license("MPL-2.0"), Some("mpl20"));
        assert_eq!(get_nix_license("Unknown"), None);
    }

    #[test]
    fn test_format_license_output_single() {
        let mut scores = BTreeMap::new();
        scores.insert("MIT".to_string(), 0.95);

        let result = format_license_output(&scores).unwrap();
        assert_eq!(result, Some("lib.licenses.mit".to_string()));
    }

    #[test]
    fn test_format_license_output_multiple() {
        let mut scores = BTreeMap::new();
        scores.insert("MIT".to_string(), 0.95);
        scores.insert("Apache-2.0".to_string(), 0.90);

        let result = format_license_output(&scores).unwrap();
        assert_eq!(
            result,
            Some("with lib.licenses; [ asl20 mit ]".to_string())
        );
    }

    #[test]
    fn test_format_license_output_empty() {
        let scores = BTreeMap::new();
        let result = format_license_output(&scores).unwrap();
        assert_eq!(result, None);
    }
}

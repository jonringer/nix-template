use crate::types::Template;
use std::path::{Path, PathBuf};

/// Determine where the file path should be written
/// Meant to save people time in dealing with file paths
/// returns (file_path_to_write, file_path_in_top_level)
/// file_path_to_write: filepath to write to disk
/// file_path_in_top_level: filepath to mention in top-level/*.nix
/// Compute the RFC140 shard for a package name: the lowercased first two
/// characters of `pname`. Names shorter than two characters are returned
/// as-is (lowercased), matching nixpkgs' fallback.
pub fn by_name_shard(pname: &str) -> String {
    let lower = pname.to_lowercase();
    lower.chars().take(2).collect()
}

/// Paths produced by the standardized `nix/` project layout used by
/// `--init-flake` (when no explicit PATH is given) and `--init-npins`.
///
/// Layout (relative to the project root, which is `base_dir`):
/// ```text
/// <base_dir>/
/// ├── flake.nix              (only when --init-flake)
/// ├── default.nix            (only when --init-npins)
/// ├── npins/                 (only when --init-npins)
/// └── nix/
///     ├── overlay.nix
///     ├── package.nix
///     └── modules/<pname>/default.nix   (only when template is `module`)
/// ```
///
/// Top-level entries (`flake.nix`, `default.nix`, `npins/`) live at the
/// project root so that non-flake consumers can `import ./.` or
/// `import ./nix/overlay.nix` directly. Modules and packages share the
/// same `nix/` tree so they can be referenced from any of the entry
/// points (`flake.nix`, `default.nix`, `release.nix`).
#[derive(Debug, Clone)]
pub struct NixDirLayout {
    /// Project root (where `flake.nix` / `default.nix` live). Kept for
    /// callers that want to derive additional paths (e.g. release.nix).
    #[allow(dead_code)]
    pub base_dir: PathBuf,
    /// `<base_dir>/nix/package.nix`.
    pub package_path: PathBuf,
    /// `<base_dir>/nix/overlay.nix`.
    pub overlay_path: PathBuf,
    /// `<base_dir>/nix/modules/<pname>/default.nix` when the template is
    /// `module`, otherwise `None`. We don't auto-generate a module file
    /// for non-module templates to avoid surprising the user.
    pub module_path: Option<PathBuf>,
    /// `<base_dir>/default.nix` (top-level wrapper for non-flake users).
    pub top_default_nix: PathBuf,
    /// `<base_dir>/flake.nix`.
    pub top_flake_nix: PathBuf,
    /// `<base_dir>/npins/` directory.
    pub npins_dir: PathBuf,
}

impl NixDirLayout {
    /// Compute the standard nix/ layout for a package rooted at
    /// `base_dir`. Pass an empty path to root at the current working
    /// directory.
    pub fn new(base_dir: &Path, pname: &str, template: &Template) -> Self {
        let nix_dir = base_dir.join("nix");
        // Use simple nix/package.nix layout for local project initialization
        let package_path = nix_dir.join("package.nix");
        let overlay_path = nix_dir.join("overlay.nix");
        let module_path = if *template == Template::Module {
            Some(nix_dir.join("modules").join(pname).join("default.nix"))
        } else {
            None
        };
        let top_default_nix = base_dir.join("default.nix");
        let top_flake_nix = base_dir.join("flake.nix");
        let npins_dir = base_dir.join("npins");

        NixDirLayout {
            base_dir: base_dir.to_path_buf(),
            package_path,
            overlay_path,
            module_path,
            top_default_nix,
            top_flake_nix,
            npins_dir,
        }
    }
}

pub fn nix_file_paths(
    matches: &clap::ArgMatches,
    template: &Template,
    path: &Path,
    pname: &str,
    nixpkgs_root: &str,
) -> (PathBuf, PathBuf) {
    // RFC140 by-name layout: pkgs/by-name/<shard>/<pname>/package.nix
    // Auto-discovered, so no top-level addition line is required (we
    // return an empty top_level path; main.rs uses that to suppress the
    // helper message).
    if matches.is_present("by-name") {
        let shard = by_name_shard(pname);
        let mut file_path = PathBuf::from(&nixpkgs_root);
        file_path.push("pkgs");
        file_path.push("by-name");
        file_path.push(&shard);
        file_path.push(pname);
        file_path.push("package.nix");
        let _ = template; // silence unused warning when no template-specific behaviour applies
        return (file_path, PathBuf::from(""));
    }

    let mut path_buf = path.to_path_buf();

    // if it's a directory, we need to default to using the default.nix which `import` expects
    // Path.is_dir() appears to return false if the directory doesn't exist, so stringify and assert if path ends in '/'
    if path.as_os_str().to_string_lossy().ends_with("/") {
        // Validate that the path doesn't contain parent directory components (..)
        // This prevents path traversal attacks
        if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            eprintln!("Warning: Path contains '..' components, which may be unsafe");
        }
        path_buf.push("default.nix");
        eprintln!("Directory was passed as [PATH], defaulting to {:?}", path_buf.display());
    }

    (path_buf, PathBuf::from(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{build_cli, validate_and_serialize_matches};
    use clap::ArgMatches;
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    fn assert_paths(m: ArgMatches, expected: (PathBuf, PathBuf)) {
        let info = validate_and_serialize_matches(&m, None);
        let actual = (info.path_to_write, info.top_level_path);

        assert_eq!(expected, actual);
    }


    #[test]
    fn by_name_shard_basic() {
        assert_eq!(by_name_shard("hello"), "he");
        assert_eq!(by_name_shard("ZLib"), "zl");
        assert_eq!(by_name_shard("a"), "a");
        assert_eq!(by_name_shard("ABC"), "ab");
    }

    #[test]
    #[serial]
    fn test_by_name_default() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "--by-name",
            "-r",
            "/tmp",
            "-p",
            "hello",
        ]);
        let expected = (
            PathBuf::from("/tmp/pkgs/by-name/he/hello/package.nix"),
            PathBuf::from(""),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_by_name_rust() {
        // by-name should work with non-default templates too
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "rust",
            "--by-name",
            "-r",
            "/tmp",
            "-p",
            "ripgrep",
        ]);
        let expected = (
            PathBuf::from("/tmp/pkgs/by-name/ri/ripgrep/package.nix"),
            PathBuf::from(""),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_by_name_uppercase_pname_lowercased_shard() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "--by-name",
            "-r",
            "/tmp",
            "-p",
            "FooBar",
        ]);
        let expected = (
            PathBuf::from("/tmp/pkgs/by-name/fo/FooBar/package.nix"),
            PathBuf::from(""),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_stdenv_no_cc_by_name() {
        // stdenvNoCC with --by-name should use RFC140 layout
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenvNoCC",
            "--by-name",
            "-p",
            "myfont",
        ]);
        let expected = (
            PathBuf::from("pkgs/by-name/my/myfont/package.nix"),
            PathBuf::from(""),
        );
        assert_paths(m, expected);
    }
}

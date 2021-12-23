use crate::types::Template;
use std::path::{Path, PathBuf};
use std::process::exit;

/// Determine where the file path should be written
/// Meant to save people time in dealing with file paths
/// returns (file_path_to_write, file_path_in_top_level)
/// file_path_to_write: filepath to write to disk
/// file_path_in_top_level: filepath to mention in top-level/*.nix
pub fn nix_file_paths(
    matches: &clap::ArgMatches,
    template: &Template,
    path: &Path,
    pname: &str,
    nixpkgs_root: &str,
) -> (PathBuf, PathBuf) {
    if matches.is_present("nixpkgs") {

        let mut radix: PathBuf;
        if matches.occurrences_of("PATH") == 0 {
            // default to nixpkgs path
            if *template == Template::python {
                eprintln!("No [PATH] provided, defaulting to \"pkgs/development/python-modules/\"");
                radix = PathBuf::from("development/python-modules/");
            } else if *template == Template::test {
                eprintln!("No [PATH] provided, defaulting to \"nixos/tests/\"");
                radix = PathBuf::from("nixos/tests/");
                radix.push(format!("{}.nix", &pname));
                return (radix, PathBuf::from(format!("./{}.nix", &pname)));
            } else if *template == Template::module {
                eprintln!("A path is required when using the module template.");
                eprintln!("For example, 'nixos/modules/services/{}.nix", &pname);
                exit(1);
            } else {
                eprintln!("No [PATH] provided, defaulting to \"pkgs/applications/misc/\"");
                radix = PathBuf::from("applications/misc");
            }
        } else {
            radix = path.strip_prefix("pkgs").unwrap_or(path).to_path_buf();
        }

        // modules just need the file location
        if *template == Template::module {
            radix = path.strip_prefix(".").unwrap_or(path).to_path_buf();
            radix = radix.strip_prefix("/").unwrap_or(&radix).to_path_buf();
            radix = radix
                .strip_prefix("nixos/modules")
                .unwrap_or(&radix)
                .to_path_buf();

            let mut module = PathBuf::from("./");
            module.push(&radix);

            let mut file_path = PathBuf::from(&nixpkgs_root);
            file_path.push("nixos");
            file_path.push("modules");
            file_path.push(&radix);

            return (file_path, module);
        }

        if !radix.ends_with(&pname) && radix.extension() != Some(std::ffi::OsStr::new("nix")) {
            radix.push(&pname);
        }

        // nix_path is the path used in pkgs/top-level/*.nix or nixos/tests/all-tests.nix
        let mut nix_path = PathBuf::from("..");
        nix_path.push(&radix);

        // file path is the path to the nix expression from NIXPKGS_ROOT
        let mut file_path = PathBuf::from(&nixpkgs_root);
        file_path.push("pkgs");
        file_path.push(&radix);

        // may have specified a specific nix file (E.g path/package.nix)
        if file_path.is_dir() || file_path.extension() != Some(std::ffi::OsStr::new("nix")) {
            file_path = file_path.join("default.nix");
        }

        return (file_path, nix_path);
    }

    (path.to_path_buf(), PathBuf::from(""))
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
    #[serial]
    fn test_module() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "module",
            "-n",
            "-p",
            "my-package",
            "nixos/modules/services/my-package.nix",
        ]);
        let expected = (
            PathBuf::from("nixos/modules/services/my-package.nix"),
            PathBuf::from("./services/my-package.nix"),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_module_long_path() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "module",
            "-n",
            "-p",
            "my-package",
            "nixos/modules/services/some-pkg-set/my-package.nix",
        ]);
        let expected = (
            PathBuf::from("nixos/modules/services/some-pkg-set/my-package.nix"),
            PathBuf::from("./services/some-pkg-set/my-package.nix"),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_test() {
        let m = build_cli().get_matches_from(vec!["nix-template", "test", "-n", "-p", "newpkg"]);
        let expected = (
            PathBuf::from("nixos/tests/newpkg.nix"),
            PathBuf::from("./newpkg.nix"),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_python() {
        let m =
            build_cli().get_matches_from(vec!["nix-template", "python", "-n", "-p", "requests"]);
        let expected = (
            PathBuf::from("pkgs/development/python-modules/requests/default.nix"),
            PathBuf::from("../development/python-modules/requests"),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_stdenv_no_path() {
        let m =
            build_cli().get_matches_from(vec!["nix-template", "stdenv", "-n", "-p", "mypackage"]);
        let expected = (
            PathBuf::from("pkgs/applications/misc/mypackage/default.nix"),
            PathBuf::from("../applications/misc/mypackage"),
        );
        assert_paths(m, expected);
    }

    #[test]
    #[serial]
    fn test_stdenv_path() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "-n",
            "-p",
            "mypackage",
            "pkgs/compilers/test/",
        ]);
        let expected = (
            PathBuf::from("pkgs/compilers/test/mypackage/default.nix"),
            PathBuf::from("../compilers/test/mypackage"),
        );
        assert_paths(m, expected);
    }
}

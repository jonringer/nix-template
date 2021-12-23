use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use crate::file_path::nix_file_paths;
use crate::types::{ExpressionInfo, Fetcher, Template, UserConfig};
use crate::url::read_meta_from_url;

// clap will validate inputs, only use on functions with possible_values defined
pub fn arg_to_type<T>(arg: Option<&str>) -> T
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    arg.unwrap().parse::<T>().unwrap()
}

// There is the assert macro, but the panic output does not look great
pub fn assert(pred: bool, message: &str) {
    if !pred {
        eprintln!("{}", message);
        std::process::exit(1);
    }
}

pub fn build_cli() -> App<'static, 'static> {
    App::new("nix-template")
        .version("0.1.4")
        .author("Jon Ringer <jonringer117@gmail.com>")
        .about("Create common nix expressions")
        .version_short("V")
        .setting(AppSettings::ColoredHelp)
        // make completions and other subcommands distinct from
        // default template usage
        .setting(AppSettings::SubcommandsNegateReqs)
        // make it so that completions subcommand doesn't
        // inherit global options
        .setting(AppSettings::ArgsNegateSubcommands)
        .after_help(
            "ENV VARS:

    GITHUB_TOKEN\tToken used during github api calls.

EXAMPLES:

# generate an expression for this package
$ nix-template rust --from-url https://github.com/jonringer/nix-template

# generate a python package expressison at pkgs/development/python-modules/requests/default.nix
$ nix-template python --nixpkgs --pname requests

# generate a shell.nix in $PWD
$ nix-template mkshell

# set maintainer name and location of nixpkgs, only needs to be set once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root ~/nixpkgs

",
        )
        .arg(
            Arg::from_usage("<TEMPLATE> 'Language or framework template target'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("stdenv"),
        )
        .arg(
            Arg::from_usage("[PATH] 'directory or file to be written. In the case of a directory, a default.nix will be created. When used with --nixpkgs, it will be appended to nixpkgs-root to determine path location.'")
                .default_value("default.nix")
                .default_value_if("TEMPLATE", Some("mkshell"), "shell.nix")
                .default_value_if("TEMPLATE", Some("test"), "test.nix")
                .default_value_if("TEMPLATE", Some("flake"), "flake.nix"),
        )
        .arg(Arg::from_usage(
            "-u,--from-url [url] 'Point to a github repo, and use github api to determine package values'",
            ))
        .arg(Arg::from_usage(
            "-l,--license [license] 'Set license'",
            ).default_value("CHANGE"))
        .arg(Arg::from_usage(
            "-m,--maintainer [maintainer] 'Set maintainer'",
            ))
        .arg(Arg::from_usage(
            "--no-meta 'Don't include meta section'",
            ).conflicts_with("nixpkgs"))
        .arg(Arg::from_usage(
            "-d,--documentation-links 'Add comments linking to relevant sections of the Nixpkgs contributor guide.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "-s,--stdout 'Write expression to stdout, instead of PATH'",
            ))
        .arg(Arg::from_usage(
            "-v [version] 'Set version of package'",
            ).default_value("0.0.1"))
        .arg(Arg::from_usage(
            "-p,--pname [pname] 'Package name to be used in expression'",
            ).default_value("CHANGE"))
        .arg(Arg::from_usage(
            "-r,--nixpkgs-root [path] 'Set root of the nixpkgs directory'",
            ).env("NIXPKGS_ROOT"))
        .arg(Arg::from_usage(
            "-n,--nixpkgs 'Intended be used within nixpkgs, will append pname to file path, and print addition statement'",
        ).takes_value(false))
        .arg(
            Arg::from_usage("-f,--fetcher [fetcher] 'Fetcher to use'")
                .possible_values(&Fetcher::variants())
                .case_insensitive(true)
                .default_value("github")
                .default_value_if("TEMPLATE", Some("python"), "pypi"),
        )
        .subcommand(
            SubCommand::with_name("completions")
                .about("Generate shell completion scripts, writes to stdout")
                .arg(
                    Arg::from_usage("<SHELL>")
                        .case_insensitive(true)
                        .possible_values(&clap::Shell::variants()),
                ),
        )
        .subcommand(
            SubCommand::with_name("config")
                .about("Set information about nix-template usage. Writes to $XDG_CONFIG_HOME")
                .arg(
                    Arg::from_usage("-f,--file [config-file] 'Config file location. [default: $XDG_CONFIG_HOME/nix-template/config.toml]'")
                )
                .subcommand(
                    SubCommand::with_name("name")
                    .about("Set maintainer name")
                    .arg(Arg::from_usage("<name>"))
                )
                .subcommand(
                    SubCommand::with_name("nixpkgs-root")
                    .about("Set the root directory of nixpkgs")
                    .arg(Arg::from_usage("<nixpkgs-root>"))
                )
        )
}

pub fn validate_and_serialize_matches(
    matches: &ArgMatches,
    user_config: Option<&UserConfig>,
) -> ExpressionInfo {
    let template: Template = arg_to_type(matches.value_of("TEMPLATE"));
    let fetcher: Fetcher = arg_to_type(matches.value_of("fetcher"));
    let pname: String = arg_to_type(matches.value_of("pname"));
    let version: String = arg_to_type(matches.value_of("v"));
    let license: String = arg_to_type(matches.value_of("license"));
    let path_str: String = arg_to_type(matches.value_of("PATH"));
    let path = std::path::PathBuf::from(&path_str);
    let include_documentation_links: bool = matches.is_present("documentation-links");
    let include_meta: bool = !matches.is_present("no-meta");

    let maintainer: String;
    let nixpkgs_root: String;
    if let Some(ref config) = user_config {
        maintainer = matches
            .value_of("maintainer")
            .or_else(|| config.maintainer.as_deref())
            .unwrap_or("")
            .to_owned();
        nixpkgs_root = matches
            .value_of("nixpkgs-root")
            .or_else(|| config.nixpkgs_root.as_deref())
            .unwrap_or("")
            .to_owned();
    } else {
        maintainer = matches.value_of("maintainer").unwrap_or("").to_string();
        nixpkgs_root = matches.value_of("nixpkgs-root").unwrap_or("").to_string();
    };

    if template == Template::flake {
        assert(matches.occurrences_of("pname") != 0, "Must provide value for -p,--pname when using flake template.");
    }

    let (path_to_write, top_level_path) =
        nix_file_paths(&matches, &template, &path, &pname, &nixpkgs_root);

    let mut info = ExpressionInfo {
        pname,
        version,
        license,
        maintainer,
        template,
        fetcher,
        path_to_write,
        top_level_path,
        include_documentation_links,
        include_meta,
        tag_prefix: "".to_owned(),
        owner: "CHANGE".to_owned(),
        src_sha: "0000000000000000000000000000000000000000000000000000".to_owned(),
        description: "CHANGE".to_owned(),
        homepage: "https://github.com/@owner@/@pname@".to_owned(),
    };

    if let Some(url) = matches.value_of("from-url") {
        read_meta_from_url(url, &mut info);
    }

    assert(!(matches.is_present("nixpkgs") && matches.occurrences_of("pname") == 0 && matches.occurrences_of("url") == 0),
        "'-p,--pname' or '-u,--from-url' is required when using the -n,--nixpkgs flag");

    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    #[test]
    fn test_python() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "python",
            "-r",
            "/tmp",
            "-n",
            "-p",
            "requests",
        ]);
        println!("{:?}", m);
        assert_eq!(m.value_of("pname"), Some("requests"));
        assert_eq!(m.value_of("TEMPLATE"), Some("python"));
        assert_eq!(m.value_of("fetcher"), Some("pypi"));
        assert_eq!(m.value_of("v"), Some("0.0.1"));
        assert_eq!(m.value_of("license"), Some("CHANGE"));
        assert_eq!(m.value_of("nixpkgs-root"), Some("/tmp"));
        assert_eq!(m.is_present("stdout"), false);
        assert_eq!(m.occurrences_of("PATH"), 0);
        assert!(m.occurrences_of("nixpkgs") >= 1);
    }

    #[test]
    fn test_mkshell() {
        let m = build_cli().get_matches_from(vec!["nix-template", "-s", "mkshell"]);
        assert_eq!(m.is_present("stdout"), true);
        assert_eq!(m.value_of("TEMPLATE"), Some("mkshell"));
        assert_eq!(m.value_of("PATH"), Some("shell.nix"));
    }

    #[test]
    fn test_test() {
        let m = build_cli().get_matches_from(vec!["nix-template", "test", "-m", "myself"]);
        assert_eq!(m.value_of("TEMPLATE"), Some("test"));
        assert_eq!(m.value_of("PATH"), Some("test.nix"));
        assert_eq!(m.value_of("maintainer"), Some("myself"));
    }

    #[test]
    fn test_fetcher() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "-f",
            "gitlab",
            "-l",
            "mit",
            "stdenv",
            "default.nix",
        ]);
        assert_eq!(m.value_of("license"), Some("mit"));
        assert_eq!(m.value_of("PATH"), Some("default.nix"));
        assert_eq!(m.occurrences_of("PATH"), 1);
        assert_eq!(m.value_of("fetcher"), Some("gitlab"));
    }

    #[test]
    #[serial] // touching global env, ensure serial runs
    fn test_nixpkgs() {
        use std::env::{remove_var, set_var};
        set_var("NIXPKGS_ROOT", "/testdir/");
        let m = build_cli().get_matches_from(vec!["nix-template", "-n"]);
        assert_eq!(m.value_of("nixpkgs-root"), Some("/testdir/"));
        remove_var("NIXPKGS_ROOT");
    }
}

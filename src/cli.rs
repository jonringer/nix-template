use clap::{App, AppSettings, Arg, SubCommand};

use crate::types::{Fetcher, Template};

pub fn build_cli() -> App<'static, 'static> {
    App::new("nix-template")
        .version("0.1")
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
            "EXAMPLES:

# generate a python package expressison at pkgs/development/python-modules/requests/default.nix
$ nix-template python --nixpkgs --pname requests

# generate a shell.nix in $PWD
$ nix-template mkshell

",
        )
        .arg(
            Arg::from_usage("<TEMPLATE> 'Language or framework template target'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("stdenv"),
        )
        .arg(
            Arg::from_usage("[PATH] 'location for file to be written'")
                .default_value("default.nix")
                .default_value_if("TEMPLATE", Some("mkshell"), "shell.nix"),
        )
        .arg(Arg::from_usage(
            "-m,--maintainer <maintainer> 'Set maintainer'",
            ).default_value("CHANGE"))
        .arg(Arg::from_usage(
            "-s,--stdout 'Write expression to stdout, instead of PATH'",
            ))
        .arg(Arg::from_usage(
            "-v <version> 'Set version of package'",
            ).default_value("0.0.1"))
        .arg(Arg::from_usage(
            "-p,--pname [pname] 'Package name to be used in expresion'",
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::{assert_eq};
    use serial_test::serial;

    #[test]
    fn test_python() {
        let m =
            build_cli().get_matches_from(vec!["nix-template", "python", "-n", "-p", "requests"]);
        println!("{:?}", m);
        assert_eq!(m.value_of("pname"), Some("requests"));
        assert_eq!(m.value_of("TEMPLATE"), Some("python"));
        assert_eq!(m.value_of("fetcher"), Some("pypi"));
        assert_eq!(m.value_of("v"), Some("0.0.1"));
        assert_eq!(m.is_present("stdout"), false);
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
    fn test_fetcher() {
        let m = build_cli().get_matches_from(vec!["nix-template", "-f", "gitlab"]);
        assert_eq!(m.value_of("fetcher"), Some("gitlab"));
    }

    #[test]
    #[serial] // touching global env, ensure serial runs
    fn test_nixpkgs() {
        use std::env::set_var;
        set_var("NIXPKGS_ROOT", "/testdir/");
        let m = build_cli().get_matches_from(vec!["nix-template", "-n"]);
        assert_eq!(m.value_of("nixpkgs-root"), Some("/testdir/"));
    }
}

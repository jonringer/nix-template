use clap::{Arg, App, SubCommand};

fn main() {
    let matches = App::new("nix-template")
                    .version("0.1")
                    .about("Create common nix expressions")
                    .after_help(
"EXAMPLES:

$ nix-template python -pname requests -f pypi pkgs/development/python-modules/

")
                    //.subcommand(SubCommand::with_name("go"))
                    //.subcommand(SubCommand::with_name("stdenv"))
                    //.subcommand(SubCommand::with_name("python"))
                    //.subcommand(SubCommand::with_name("rust"))
                    .arg(Arg::from_usage("<TEMPLATE> 'Language or framework template target'")
                         .possible_values(&["stdenv","go","mkshell","python","rust"])
                         .default_value("stdenv"))
                    .arg(Arg::from_usage("[PATH] 'location for file to be written'").default_value("default.nix"))
                    .arg(Arg::from_usage("-p,--pname [pname] 'Package name to be used in expresion'"))
                    .arg(Arg::from_usage("-f,--fetcher [fetcher] 'Fetcher to use'")
                        .possible_values(&["github", "gitlab", "git","url","zip","pypi"])
                        .default_value("github"))
                    .get_matches();
                    //.get_matches_from(vec!["go", "--pname", "direnv"]);
    println!("{:?}", matches);
    assert_eq!(matches.value_of("pname"), Some("direnv"));
}

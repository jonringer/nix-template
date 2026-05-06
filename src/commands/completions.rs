use crate::cli;

pub fn run(matches: &clap::ArgMatches) {
    // clap would have failed if a valid shell str wasn't passed
    cli::build_cli().gen_completions_to(
        "nix-template",
        cli::arg_to_type::<clap::Shell>(matches.value_of("SHELL")),
        &mut std::io::stdout(),
    )
}

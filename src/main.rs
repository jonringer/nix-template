mod cli;
mod types;

use std::io;
use clap::Shell;

fn main() {
    let m = cli::build_cli().get_matches();
    println!("{:?}", m);

    match m.subcommand() {
        ("completions", Some(m)) => { cli::build_cli().gen_completions_to("nix-template", m.value_of("SHELL").unwrap().parse::<Shell>().unwrap(), &mut io::stdout())},
        _ => {},
    }
}

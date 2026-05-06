#[macro_use]
extern crate lazy_static;

mod cli;
mod commands;
mod deps;
mod detect;
mod expression;
mod file_path;
mod interactive;
mod output;
mod source;
mod types;
mod url;

use types::UserConfig;

fn main() {
    env_logger::init();

    let xdg_dirs = xdg::BaseDirectories::with_prefix("nix-template").unwrap();

    let user_config: Option<UserConfig> =
        if let Some(file) = xdg_dirs.find_config_file("config.toml") {
            toml::from_str(&std::fs::read_to_string(file).unwrap()).ok()
        } else {
            None
        };

    let matches = cli::build_cli().get_matches();

    match matches.subcommand() {
        ("completions", Some(m)) => {
            commands::completions::run(m);
        }
        ("config", Some(m)) => {
            commands::config::run(m, &xdg_dirs, user_config);
        }
        _ => {
            commands::build::run(&matches, &xdg_dirs, user_config.as_ref());
        }
    }
}

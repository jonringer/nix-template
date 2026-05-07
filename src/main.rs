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

    // Attempt to set up XDG directories; warn and continue if it fails
    let xdg_dirs = match xdg::BaseDirectories::with_prefix("nix-template") {
        Ok(dirs) => dirs,
        Err(e) => {
            eprintln!("Warning: Unable to access config directory: {}", e);
            eprintln!("Continuing without user configuration...");
            // Create a fallback with current directory to allow the program to run
            xdg::BaseDirectories::new().unwrap_or_else(|err| {
                eprintln!("Error: Cannot initialize XDG directories: {}", err);
                std::process::exit(1);
            })
        }
    };

    // Attempt to load user config; warn and continue if it fails
    let user_config: Option<UserConfig> =
        if let Some(file) = xdg_dirs.find_config_file("config.toml") {
            match std::fs::read_to_string(&file) {
                Ok(contents) => {
                    toml::from_str(&contents).map_err(|e| {
                        eprintln!("Warning: Could not parse config file {:?}: {}", file, e);
                        eprintln!("Continuing without user configuration...");
                    }).ok()
                }
                Err(e) => {
                    eprintln!("Warning: Could not read config file {:?}: {}", file, e);
                    eprintln!("Continuing without user configuration...");
                    None
                }
            }
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

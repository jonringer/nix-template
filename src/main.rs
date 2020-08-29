#[macro_use]
extern crate lazy_static;

mod cli;
mod expression;
mod file_path;
mod types;

use cli::arg_to_type;
use types::UserConfig;

fn main() {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("nix-template").unwrap();

    let mut user_config: Option<UserConfig> = if let Some(file) = xdg_dirs.find_config_file("config.toml") {
        toml::from_str(&std::fs::read_to_string(file).unwrap()).ok()
    } else {
        None
    };

    let m = cli::build_cli().get_matches();

    match m.subcommand() {
        ("completions", Some(m)) => {
            // clap would have failed if a valid shell str wasn't passed
            cli::build_cli().gen_completions_to(
                "nix-template",
                arg_to_type::<clap::Shell>(m.value_of("SHELL")),
                &mut std::io::stdout(),
            )
        }
        ("config", Some(m)) => {
            let config_path = xdg_dirs.place_config_file("config.toml")
                .unwrap_or_else(|_| panic!("unable to create configuration directory"));

            // set config
            match m.subcommand() {
                ("name", Some(m)) => {
                    //set name
                    let name: Option<String> = m.value_of("name").map(|s| s.to_string());

                    // since we can only set 1 value currently, this is a bit of over-engineered
                    // however, we want to prevent overriding future values
                    if let Some(ref mut config) = user_config {
                        config.maintainer = name;
                    } else {
                        user_config = Some(UserConfig { maintainer: name, nixpkgs_root: None })
                    };
                }
                ("nixpkgs-root", Some(m)) => {
                    //set nixpkgs root
                    let root: Option<String> = m.value_of("nixpkgs-root").map(|s| s.to_string());

                    // since we can only set 1 value currently, this is a bit of over-engineered
                    // however, we want to prevent overriding future values
                    if let Some(ref mut config) = user_config {
                        config.nixpkgs_root = root;
                    } else {
                        user_config = Some(UserConfig { maintainer: None, nixpkgs_root: root })
                    };
                }
                _ => {
                    eprintln!("Unexpected command given to config subcommand.");
                    std::process::exit(1);
                }
            }

            // write config
            std::fs::write(&config_path, toml::to_string(&user_config).unwrap())
                .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &config_path.display()));
        }
        _ => {
            // build expression
            let user_config: Option<UserConfig> = if let Some(file) = xdg_dirs.find_config_file("config.toml") {
                toml::from_str(&std::fs::read_to_string(file).unwrap()).ok()
            } else {
                None
            };

            let info = cli::validate_and_serialize_matches(&m, user_config.as_ref());

            let expr = expression::generate_expression(&info);

            let path = &info.path_to_write;

            if path.exists() {
                eprintln!("Cannot write to file '{}', already exists", path.display());
                std::process::exit(1);
            }

            if m.is_present("stdout") {
                println!("{}", expr);
            } else {
                // ensure directory to file exists
                if let Some(p) = path.parent() {
                    if !path.exists() {
                        println!("Creating directory: {}", p.display());
                        std::fs::create_dir_all(p)
                            .unwrap_or_else(|_| panic!("Was unable to create directory {}", p.display()));
                    }
                }
                std::fs::write(path, expr)
                    .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &path.display()));
            }
        }
    }
}

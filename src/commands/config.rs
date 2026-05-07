use crate::types::UserConfig;

pub fn run(
    matches: &clap::ArgMatches,
    xdg_dirs: &xdg::BaseDirectories,
    mut user_config: Option<UserConfig>,
) {
    let config_path = xdg_dirs
        .place_config_file("config.toml")
        .unwrap_or_else(|_| panic!("unable to create configuration directory"));

    // set config
    match matches.subcommand() {
        ("name", Some(m)) => {
            //set name
            let name: Option<String> = m.value_of("name").map(|s| s.to_string());

            // since we can only set 1 value currently, this is a bit of over-engineered
            // however, we want to prevent overriding future values
            if let Some(ref mut config) = user_config {
                config.maintainer = name;
            } else {
                user_config = Some(UserConfig {
                    maintainer: name,
                    nixpkgs_root: None,
                })
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
                user_config = Some(UserConfig {
                    maintainer: None,
                    nixpkgs_root: root,
                })
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

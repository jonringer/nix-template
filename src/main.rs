#[macro_use]
extern crate lazy_static;

mod cli;
mod expression;
mod file_path;
mod go_deps;
mod interactive;
mod rust_deps;
mod types;
mod url;

use cli::arg_to_type;
use types::{Template, UserConfig};

fn main() {
    env_logger::init();

    let xdg_dirs = xdg::BaseDirectories::with_prefix("nix-template").unwrap();

    let mut user_config: Option<UserConfig> =
        if let Some(file) = xdg_dirs.find_config_file("config.toml") {
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
            let config_path = xdg_dirs
                .place_config_file("config.toml")
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
            std::fs::write(&config_path, toml::to_string(&user_config).unwrap()).unwrap_or_else(
                |_| panic!("Was unable to write to file: {}", &config_path.display()),
            );
        }
        _ => {
            // build expression
            let user_config: Option<UserConfig> =
                if let Some(file) = xdg_dirs.find_config_file("config.toml") {
                    toml::from_str(&std::fs::read_to_string(file).unwrap()).ok()
                } else {
                    None
                };

            // Detect if we should enter interactive mode
            // Enter interactive mode if:
            // 1. Template was not explicitly provided (using default)
            // 2. AND no URL was provided
            // 3. AND pname is still "CHANGE"
            let should_use_interactive = m.occurrences_of("TEMPLATE") == 0
                && m.occurrences_of("from-url") == 0
                && m.value_of("pname") == Some("CHANGE");

            let mut info = if should_use_interactive {
                // Enter interactive mode
                match interactive::run_interactive_mode(None, user_config.as_ref()) {
                    Ok(interactive_data) => {
                        cli::build_expression_info_from_interactive(
                            interactive_data,
                            user_config.as_ref(),
                        )
                    }
                    Err(e) => {
                        eprintln!("Interactive mode cancelled or failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // Use traditional CLI mode
                cli::validate_and_serialize_matches(&m, user_config.as_ref())
            };

            // Handle --init-npins flag: if the package would be written
            // as `default.nix`, the wrapper we want to generate would
            // collide with it. Rename the package output to `package.nix`
            // (mirrors the RFC140 by-name convention) so the wrapper can
            // own `default.nix`.
            let init_npins = m.is_present("init-npins");
            if init_npins {
                let needs_rename = info
                    .path_to_write
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "default.nix")
                    .unwrap_or(false);
                if needs_rename {
                    let new_path = if let Some(parent) = info.path_to_write.parent() {
                        parent.join("package.nix")
                    } else {
                        std::path::PathBuf::from("package.nix")
                    };
                    println!(
                        "--init-npins: writing package as 'package.nix' to leave 'default.nix' \
for the npins wrapper."
                    );
                    info.path_to_write = new_path;
                }
            }

            let expr = expression::generate_expression(&info);
            let output = info.format(&expr);

            // Handle --init-flake flag
            let init_flake = m.is_present("init-flake");
            let flake_content = if init_flake {
                // Get current directory name for flake description
                let cwd = std::env::current_dir().expect("Failed to get current directory");
                let directory_name = cwd
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("CHANGE");

                // Get the output filename (not full path)
                let output_filename = info.path_to_write
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("default.nix");

                Some(expression::generate_flake_nix(&info.template, output_filename, directory_name))
            } else {
                None
            };

            // Handle --init-npins flag. The wrapper default.nix and the
            // npins/ directory both live alongside the package file.
            let npins_payload = if init_npins {
                let parent = info
                    .path_to_write
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from(""));

                let npins_dir = parent.join("npins");
                let npins_default_path = npins_dir.join("default.nix");
                let npins_sources_path = npins_dir.join("sources.json");
                let wrapper_path = parent.join("default.nix");

                // Wrapper imports the package by its on-disk basename
                // (which may have been renamed to `package.nix` above).
                let package_basename = info
                    .path_to_write
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("package.nix")
                    .to_string();

                let wrapper_content =
                    expression::generate_npins_wrapper_default_nix(&info.template, &package_basename);

                Some((
                    npins_dir,
                    npins_default_path,
                    expression::generate_npins_default_nix().to_string(),
                    npins_sources_path,
                    expression::generate_npins_sources_json().to_string(),
                    wrapper_path,
                    wrapper_content,
                ))
            } else {
                None
            };

            if m.is_present("stdout") {
                println!("{}", output);
                if let Some(flake) = &flake_content {
                    println!("\n# ===== flake.nix =====\n");
                    println!("{}", flake);
                }
                if let Some((
                    _npins_dir,
                    npins_default_path,
                    npins_default_content,
                    npins_sources_path,
                    npins_sources_content,
                    wrapper_path,
                    wrapper_content,
                )) = &npins_payload
                {
                    println!(
                        "\n# ===== {} =====\n",
                        npins_default_path.display()
                    );
                    println!("{}", npins_default_content);
                    println!(
                        "\n# ===== {} =====\n",
                        npins_sources_path.display()
                    );
                    println!("{}", npins_sources_content);
                    println!("\n# ===== {} =====\n", wrapper_path.display());
                    println!("{}", wrapper_content);
                }
            } else {
                let path = &info.path_to_write;

                // ensure directory to file exists
                if let Some(p) = path.parent() {
                    // TODO: better way to determine that file will be written PWD
                    if p.to_str() != Some("") && !p.exists() {
                        println!("Creating directory: {}", p.display());
                        std::fs::create_dir_all(p).unwrap_or_else(|_| {
                            panic!("Was unable to create directory {}", p.display())
                        });
                    }
                }
                // write file
                std::fs::write(path, output)
                    .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &path.display()));
                println!(
                    "Generated a {} nix expression at {}",
                    &info.template,
                    &path.canonicalize().unwrap().display()
                );

                // Write flake.nix if --init-flake was provided
                if let Some(flake) = flake_content {
                    let flake_path = if let Some(parent) = path.parent() {
                        parent.join("flake.nix")
                    } else {
                        std::path::PathBuf::from("flake.nix")
                    };

                    std::fs::write(&flake_path, flake)
                        .unwrap_or_else(|_| panic!("Was unable to write to file: {}", &flake_path.display()));
                    println!(
                        "Generated flake.nix at {}",
                        &flake_path.canonicalize().unwrap().display()
                    );
                }

                // Write npins scaffold if --init-npins was provided
                if let Some((
                    npins_dir,
                    npins_default_path,
                    npins_default_content,
                    npins_sources_path,
                    npins_sources_content,
                    wrapper_path,
                    wrapper_content,
                )) = npins_payload
                {
                    // Refuse to clobber any of the three target files.
                    for p in [&npins_default_path, &npins_sources_path, &wrapper_path].iter() {
                        if p.exists() {
                            eprintln!(
                                "Refusing to overwrite existing file: {}",
                                p.display()
                            );
                            std::process::exit(1);
                        }
                    }

                    // Ensure npins/ directory exists
                    if npins_dir.to_str() != Some("") && !npins_dir.exists() {
                        println!("Creating directory: {}", npins_dir.display());
                        std::fs::create_dir_all(&npins_dir).unwrap_or_else(|_| {
                            panic!("Was unable to create directory {}", npins_dir.display())
                        });
                    }

                    std::fs::write(&npins_default_path, npins_default_content)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Was unable to write to file: {}",
                                &npins_default_path.display()
                            )
                        });
                    println!(
                        "Generated npins lockfile reader at {}",
                        &npins_default_path.canonicalize().unwrap().display()
                    );

                    std::fs::write(&npins_sources_path, npins_sources_content)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Was unable to write to file: {}",
                                &npins_sources_path.display()
                            )
                        });
                    println!(
                        "Generated empty npins/sources.json at {}",
                        &npins_sources_path.canonicalize().unwrap().display()
                    );

                    std::fs::write(&wrapper_path, wrapper_content)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Was unable to write to file: {}",
                                &wrapper_path.display()
                            )
                        });
                    println!(
                        "Generated npins wrapper default.nix at {}",
                        &wrapper_path.canonicalize().unwrap().display()
                    );

                    println!();
                    println!("Next steps:");
                    let project_dir = npins_dir
                        .parent()
                        .map(|p| {
                            let s = p.display().to_string();
                            if s.is_empty() { ".".to_string() } else { s }
                        })
                        .unwrap_or_else(|| ".".into());
                    println!(
                        "  1. cd into {} (if not already there)",
                        project_dir
                    );
                    println!("  2. Pin nixpkgs:  npins add channel nixpkgs-unstable");
                    println!("  3. Build:        nix-build");
                    println!();
                }

                // print helpful message about line to be included in pkgs/top-level
                // RFC140 (--by-name) packages are auto-discovered, so we skip
                // this hint when --by-name is set.
                if m.is_present("nixpkgs") && !m.is_present("by-name") {
                    println!("Please add the following line to the appropriate file:");
                    println!();
                    match &info.template {
                        Template::module => println!("  {}", &info.top_level_path.display()),
                        Template::test => println!(
                            "  {} = handleTest {} {{ }};",
                            &info.pname,
                            &info.top_level_path.display()
                        ),
                        _ => println!(
                            "  {} = callPackage {} {{ }};",
                            &info.pname,
                            &info.top_level_path.display()
                        ),
                    }
                    println!();
                } else if m.is_present("by-name") {
                    println!();
                    println!(
                        "RFC140 layout: '{}' will be auto-discovered from pkgs/by-name; \
no addition to all-packages.nix is required.",
                        &info.pname
                    );
                    println!();
                }
            }
        }
    }
}

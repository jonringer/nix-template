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
use file_path::NixDirLayout;
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

            // ----------------------------------------------------------------
            // Init-flag bookkeeping. We support three orthogonal init flags:
            //   --init-flake    write a top-level flake.nix
            //   --init-npins    scaffold an npins/ directory + default.nix
            //   --init-project  produce a structured nix/ layout, prompting
            //                   for the template if not given
            //
            // When the structured layout is active (always for --init-project
            // and --init-npins; opted into for --init-flake when no PATH was
            // given), files land at:
            //   ./flake.nix           (--init-flake)
            //   ./default.nix         (--init-npins or --init-project)
            //   ./npins/              (--init-npins)
            //   ./nix/overlay.nix
            //   ./nix/pkgs/<pname>/package.nix
            //   ./nix/modules/<pname>/default.nix   (module template only)
            // ----------------------------------------------------------------
            let init_flake = m.is_present("init-flake");
            let init_npins = m.is_present("init-npins");
            let init_project = m.is_present("init-project");
            let no_path_given = m.occurrences_of("PATH") == 0;

            // Decide whether to use the structured nix/ layout.
            //   - Always for --init-project, --init-npins, and --init-flake
            //     when no explicit PATH was given. (--init-flake with an
            //     explicit PATH preserves the legacy flat layout for scripts
            //     that depend on it.)
            //   - Never when --by-name is in play (it has its own canonical
            //     placement under nixpkgs).
            let nixpkgs_layout_active = m.is_present("by-name");
            let use_structured_layout = !nixpkgs_layout_active
                && (init_project || init_npins || (init_flake && no_path_given));

            // When --init-project is requested without an explicit template,
            // prompt the user to pick one. We also re-run when the legacy
            // path (template default + pname provided) didn't trigger
            // interactive mode.
            if init_project
                && m.occurrences_of("TEMPLATE") == 0
                && !should_use_interactive
            {
                match interactive::prompt_template_type(None) {
                    Ok(t) => {
                        info.template = t;
                    }
                    Err(e) => {
                        eprintln!("Template selection cancelled: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            // Compute the structured layout up front so every downstream
            // step (path rewrite, overlay, top-level wrappers, flake) can
            // reference the same set of paths.
            let layout: Option<NixDirLayout> = if use_structured_layout {
                Some(NixDirLayout::new(
                    std::path::Path::new(""),
                    &info.pname,
                    &info.template,
                ))
            } else {
                None
            };

            // Rewrite the package output path for the structured layout.
            // For module templates the package_path is unused; we redirect
            // info.path_to_write to the module file under nix/modules/.
            if let Some(ref l) = layout {
                if info.template == Template::module {
                    if let Some(ref module_path) = l.module_path {
                        info.path_to_write = module_path.clone();
                    }
                } else {
                    info.path_to_write = l.package_path.clone();
                }
            }

            // Legacy --init-npins behaviour (no structured layout — only
            // possible when --nixpkgs is set, since otherwise we always opt
            // into the structured layout above): if the package would be
            // written as `default.nix`, rename to `package.nix` so the
            // wrapper can own `default.nix`.
            if init_npins && layout.is_none() {
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

            // Helper: directory name to use in flake `description` field.
            let directory_name_owned = std::env::current_dir()
                .ok()
                .and_then(|cwd| cwd.file_name().map(|n| n.to_owned()))
                .and_then(|n| n.to_str().map(|s| s.to_owned()))
                .unwrap_or_else(|| "CHANGE".to_owned());
            let directory_name = directory_name_owned.as_str();

            // ----- flake.nix payload -----
            let flake_payload: Option<(std::path::PathBuf, String)> = if init_flake {
                if let Some(ref l) = layout {
                    Some((
                        l.top_flake_nix.clone(),
                        expression::generate_structured_flake_nix(
                            &info.template,
                            &info.pname,
                            directory_name,
                        ),
                    ))
                } else {
                    // Legacy: flake.nix sits next to the package expression.
                    let flake_path = info
                        .path_to_write
                        .parent()
                        .map(|p| p.join("flake.nix"))
                        .unwrap_or_else(|| std::path::PathBuf::from("flake.nix"));
                    let output_filename = info
                        .path_to_write
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("default.nix");
                    Some((
                        flake_path,
                        expression::generate_flake_nix(
                            &info.template,
                            output_filename,
                            directory_name,
                        ),
                    ))
                }
            } else {
                None
            };

            // ----- overlay.nix payload (structured layout only) -----
            let overlay_payload: Option<(std::path::PathBuf, String)> = layout
                .as_ref()
                .map(|l| {
                    (
                        l.overlay_path.clone(),
                        expression::generate_overlay_nix(&info.template, &info.pname),
                    )
                });

            // ----- top-level default.nix payload (structured layout only) -----
            // Emitted whenever --init-npins or --init-project is in play, so
            // that non-flake consumers have a working entry point. We skip
            // it for `--init-flake` alone since flake.nix is the only entry
            // point the user asked for in that case.
            let want_top_default = layout.is_some() && (init_npins || init_project);
            let top_default_payload: Option<(std::path::PathBuf, String)> = if want_top_default {
                layout.as_ref().map(|l| {
                    (
                        l.top_default_nix.clone(),
                        expression::generate_structured_default_nix(
                            &info.template,
                            &info.pname,
                            init_npins,
                        ),
                    )
                })
            } else {
                None
            };

            // ----- npins payload -----
            // Two flavours: structured (npins/ at project root, wrapper is
            // the top-level default.nix above) and legacy (everything next
            // to the package file, with a dedicated wrapper).
            let npins_payload = if init_npins {
                if let Some(ref l) = layout {
                    let npins_dir = l.npins_dir.clone();
                    let npins_default_path = npins_dir.join("default.nix");
                    let npins_sources_path = npins_dir.join("sources.json");
                    Some((
                        npins_dir,
                        npins_default_path,
                        expression::generate_npins_default_nix().to_string(),
                        npins_sources_path,
                        expression::generate_npins_sources_json().to_string(),
                        // For the structured layout the top-level default.nix
                        // *is* the npins-aware wrapper; pass `None` here to
                        // signal "no separate wrapper".
                        None,
                    ))
                } else {
                    let parent = info
                        .path_to_write
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| std::path::PathBuf::from(""));

                    let npins_dir = parent.join("npins");
                    let npins_default_path = npins_dir.join("default.nix");
                    let npins_sources_path = npins_dir.join("sources.json");
                    let wrapper_path = parent.join("default.nix");

                    let package_basename = info
                        .path_to_write
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("package.nix")
                        .to_string();

                    let wrapper_content = expression::generate_npins_wrapper_default_nix(
                        &info.template,
                        &package_basename,
                    );

                    Some((
                        npins_dir,
                        npins_default_path,
                        expression::generate_npins_default_nix().to_string(),
                        npins_sources_path,
                        expression::generate_npins_sources_json().to_string(),
                        Some((wrapper_path, wrapper_content)),
                    ))
                }
            } else {
                None
            };

            if m.is_present("stdout") {
                println!("{}", output);
                if let Some((flake_path, flake)) = &flake_payload {
                    println!("\n# ===== {} =====\n", flake_path.display());
                    println!("{}", flake);
                }
                if let Some((overlay_path, overlay)) = &overlay_payload {
                    println!("\n# ===== {} =====\n", overlay_path.display());
                    println!("{}", overlay);
                }
                if let Some((top_path, top_content)) = &top_default_payload {
                    println!("\n# ===== {} =====\n", top_path.display());
                    println!("{}", top_content);
                }
                if let Some((
                    _npins_dir,
                    npins_default_path,
                    npins_default_content,
                    npins_sources_path,
                    npins_sources_content,
                    legacy_wrapper,
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
                    if let Some((wrapper_path, wrapper_content)) = legacy_wrapper {
                        println!("\n# ===== {} =====\n", wrapper_path.display());
                        println!("{}", wrapper_content);
                    }
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

                // Helper to write a generated artifact, refusing to clobber
                // any pre-existing file. Creates parent directories as
                // needed.
                fn write_new(path: &std::path::Path, content: &str, label: &str) {
                    if path.exists() {
                        eprintln!(
                            "Refusing to overwrite existing file: {}",
                            path.display()
                        );
                        std::process::exit(1);
                    }
                    if let Some(parent) = path.parent() {
                        if parent.to_str() != Some("") && !parent.exists() {
                            println!("Creating directory: {}", parent.display());
                            std::fs::create_dir_all(parent).unwrap_or_else(|_| {
                                panic!("Was unable to create directory {}", parent.display())
                            });
                        }
                    }
                    std::fs::write(path, content).unwrap_or_else(|_| {
                        panic!("Was unable to write to file: {}", path.display())
                    });
                    println!(
                        "Generated {} at {}",
                        label,
                        path.canonicalize().unwrap().display()
                    );
                }

                // Write overlay.nix (structured layout only). Done before
                // flake/default so the imports referenced by those wrappers
                // exist on disk in the order a user inspecting progress
                // would expect.
                if let Some((overlay_path, overlay_content)) = &overlay_payload {
                    write_new(overlay_path, overlay_content, "overlay.nix");
                }

                // Write top-level default.nix (structured layout only).
                if let Some((top_path, top_content)) = &top_default_payload {
                    write_new(top_path, top_content, "top-level default.nix");
                }

                // Write flake.nix if --init-flake was provided.
                if let Some((flake_path, flake_content)) = &flake_payload {
                    write_new(flake_path, flake_content, "flake.nix");
                }

                // Write npins scaffold if --init-npins was provided.
                if let Some((
                    npins_dir,
                    npins_default_path,
                    npins_default_content,
                    npins_sources_path,
                    npins_sources_content,
                    legacy_wrapper,
                )) = npins_payload
                {
                    // Ensure npins/ directory exists.
                    if npins_dir.to_str() != Some("") && !npins_dir.exists() {
                        println!("Creating directory: {}", npins_dir.display());
                        std::fs::create_dir_all(&npins_dir).unwrap_or_else(|_| {
                            panic!("Was unable to create directory {}", npins_dir.display())
                        });
                    }

                    write_new(
                        &npins_default_path,
                        &npins_default_content,
                        "npins lockfile reader",
                    );
                    write_new(
                        &npins_sources_path,
                        &npins_sources_content,
                        "empty npins/sources.json",
                    );
                    if let Some((wrapper_path, wrapper_content)) = legacy_wrapper {
                        write_new(&wrapper_path, &wrapper_content, "npins wrapper default.nix");
                    }

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

                // Note: --by-name packages are auto-discovered via RFC140, so no
                // manual addition to all-packages.nix is needed.
                if m.is_present("by-name") {
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

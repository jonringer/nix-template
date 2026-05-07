use crate::{
    cli, expression,
    file_path::NixDirLayout,
    interactive, output,
    types::{Template, UserConfig},
};

pub fn run(
    matches: &clap::ArgMatches,
    _xdg_dirs: &xdg::BaseDirectories,
    user_config: Option<&UserConfig>,
) {
    // ----------------------------------------------------------------
    // Init mode detection: --init-flake and --init-npins are special
    // modes that initialize the current directory as a Nix project.
    // They should:
    // 1. Auto-detect template from local files
    // 2. Infer dependencies from local sources
    // 3. Default pname to directory name (kebab-case)
    // 4. Use local fetcher (src = ./..;)
    // 5. Enter interactive mode with smart defaults
    //
    // Init mode is only triggered when:
    // - --init-flake or --init-npins is present
    // - AND no --from-url is provided (we're working with local sources)
    // - AND either auto-detection finds project files OR explicit template is not a remote-only one
    // ----------------------------------------------------------------
    let has_init_flag = matches.is_present("init-flake") || matches.is_present("init-npins");
    let no_url = matches.occurrences_of("from-url") == 0;

    // Pre-detect to see if there are actual project files
    let has_local_project_files = if has_init_flag && no_url {
        let cwd = std::env::current_dir().unwrap_or_default();
        !crate::detect::detect_template_candidates_from_path(&cwd).is_empty()
    } else {
        false
    };

    let is_init_mode = has_init_flag && no_url && has_local_project_files;

    // Get current directory name for default pname (convert to kebab-case)
    let directory_name = if is_init_mode {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| cwd.file_name().map(|n| n.to_owned()))
            .and_then(|n| {
                n.to_str().map(|s| {
                    // Convert to kebab-case: lowercase, replace _ and spaces with -
                    s.to_lowercase().replace('_', "-").replace(' ', "-")
                })
            })
            .unwrap_or_else(|| "my-project".to_owned())
    } else {
        String::new()
    };

    // Auto-detect template, infer dependencies, and detect builder
    // variants from local directory.
    //
    // `local_use_cargo_lock_file`: Rust local mode uses cargoLock.lockFile
    // `local_go_vendor_null`: Go local mode with vendor/ uses vendorHash = null
    // `local_python_format`: Python format auto-detected from pyproject.toml
    let mut local_use_cargo_lock_file = false;
    let mut local_cargo_lock_git_deps: Vec<String> = Vec::new();
    let mut local_go_vendor_null = false;
    let mut local_go_module_path = String::new();
    let mut local_python_format: Option<String> = None;
    let mut local_python_propagated_deps: Vec<String> = Vec::new();

    let (detected_candidates, inferred_deps) = if is_init_mode {
        let cwd = std::env::current_dir().unwrap_or_default();

        // Detect template from local files
        let candidates = crate::detect::detect_template_candidates_from_path(&cwd);

        if !candidates.is_empty() {
            eprintln!("Detected project type: {}", candidates[0].template);
            if candidates.len() > 1 {
                eprintln!(
                    "Note: Multiple project types detected ({}). Using first match.",
                    candidates
                        .iter()
                        .map(|c| format!("{:?}", c.template))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }

        // Detect builder variants for local development
        if let Some(candidate) = candidates.first() {
            match candidate.template {
                crate::types::Template::Rust(_) => {
                    local_use_cargo_lock_file = true;
                    // Scan Cargo.lock for git dependencies that need outputHashes
                    let lock_path = cwd.join("Cargo.lock");
                    if let Ok(lock_content) = std::fs::read_to_string(&lock_path) {
                        local_cargo_lock_git_deps =
                            crate::deps::rust::parse_cargo_lock_git_deps(&lock_content);
                        if !local_cargo_lock_git_deps.is_empty() {
                            eprintln!(
                                "Detected {} git dependencies in Cargo.lock requiring outputHashes",
                                local_cargo_lock_git_deps.len()
                            );
                        }
                    }
                }
                crate::types::Template::Go(_) => {
                    if cwd.join("vendor").is_dir() {
                        eprintln!("Detected vendor/ directory; using vendorHash = null");
                        local_go_vendor_null = true;
                    }
                    if let Some(module) = crate::deps::go::parse_go_mod_module(&cwd) {
                        local_go_module_path = module;
                    }
                }
                crate::types::Template::Ruby => {
                    if !cwd.join("gemset.nix").exists() {
                        eprintln!(
                            "Warning: gemset.nix not found. Run 'bundix' to generate it \
                             (required by bundlerApp)."
                        );
                    }
                }
                crate::types::Template::Python(_) => {
                    let fmt = crate::detect::detect_python_format(&cwd);
                    eprintln!("Detected Python build format: {}", fmt);
                    local_python_format = Some(fmt);
                }
                _ => {}
            }
        }

        // Infer dependencies for the detected template (if any)
        // We try both template-specific inference AND build system inference
        let mut deps = if let Some(candidate) = candidates.first() {
            match candidate.template {
                crate::types::Template::Rust(_) => {
                    crate::deps::rust::infer_rust_dependencies_from_path(&cwd)
                        .unwrap_or_else(|| (Vec::new(), Vec::new()))
                }
                crate::types::Template::Go(_) => {
                    crate::deps::go::infer_go_dependencies_from_path(&cwd)
                        .unwrap_or_else(|| (Vec::new(), Vec::new()))
                }
                crate::types::Template::Ruby => {
                    crate::deps::ruby::infer_ruby_dependencies_from_path(&cwd)
                        .unwrap_or_else(|| (Vec::new(), Vec::new()))
                }
                crate::types::Template::Python(_) => {
                    local_python_propagated_deps =
                        crate::deps::python::infer_python_dependencies_from_path(&cwd);
                    (Vec::new(), Vec::new())
                }
                _ => (Vec::new(), Vec::new()),
            }
        } else {
            (Vec::new(), Vec::new())
        };

        // Always try build system inference (cmake, meson, autotools)
        // This works for any template type
        if let Some((build_inputs, native_build_inputs)) =
            crate::deps::buildsystem::infer_buildsystem_dependencies_from_path(&cwd)
        {
            deps.0.extend(build_inputs);
            deps.1.extend(native_build_inputs);
        }

        (candidates, deps)
    } else {
        (Vec::new(), (Vec::new(), Vec::new()))
    };

    // Detect if we should enter interactive mode
    // Enter interactive mode if:
    // 1. We're in init mode AND essential info is missing (pname, license, maintainer)
    // 2. OR (Template was not explicitly provided AND no URL AND pname is "CHANGE")
    let should_use_interactive = (is_init_mode
        && (matches.value_of("pname") == Some("CHANGE")
            || matches.value_of("license") == Some("CHANGE")
            || matches.value_of("maintainer") == Some("CHANGE")))
        || (matches.occurrences_of("TEMPLATE") == 0
            && matches.occurrences_of("from-url") == 0
            && matches.value_of("pname") == Some("CHANGE"));

    let mut info = if should_use_interactive {
        // Enter interactive mode (with defaults for init mode)
        let interactive_result = if is_init_mode {
            interactive::run_interactive_mode_with_defaults(
                None,
                user_config,
                detected_candidates,
                Some(directory_name.clone()),
                Some(inferred_deps),
                true, // is_local_init
            )
        } else {
            interactive::run_interactive_mode(None, user_config)
        };

        match interactive_result {
            Ok(interactive_data) => {
                cli::build_expression_info_from_interactive(interactive_data, user_config)
            }
            Err(e) => {
                eprintln!("Interactive mode cancelled or failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Use traditional CLI mode
        let mut cli_info = cli::validate_and_serialize_matches(matches, user_config);

        // Apply init mode defaults if in init mode
        if is_init_mode {
            // Use local fetcher
            cli_info.fetcher = crate::types::Fetcher::local;

            // Use detected template if not explicitly set
            if cli_info.template == crate::types::Template::Auto && !detected_candidates.is_empty()
            {
                cli_info.template = detected_candidates[0].template.clone();
            }

            // Use directory name if pname is still CHANGE
            if cli_info.pname == "CHANGE" && !directory_name.is_empty() {
                cli_info.pname = directory_name.clone();
            }

            // Apply inferred dependencies
            if !inferred_deps.0.is_empty() || !inferred_deps.1.is_empty() {
                cli_info.build_inputs = inferred_deps.0.clone();
                cli_info.native_build_inputs = inferred_deps.1.clone();
            }
        }

        cli_info
    };

    // Apply local development builder variants detected above.
    if local_use_cargo_lock_file {
        info.use_cargo_lock_file = true;
        info.cargo_lock_git_deps = local_cargo_lock_git_deps;
    }
    if local_go_vendor_null {
        info.vendor_hash = crate::types::VENDOR_HASH_NULL.to_owned();
    }
    if !local_go_module_path.is_empty() {
        info.go_module_path = local_go_module_path;
    }
    if let Some(fmt) = local_python_format {
        info.python_format = fmt;
    }
    if !local_python_propagated_deps.is_empty() {
        info.propagated_build_inputs = local_python_propagated_deps;
    }

    // ----------------------------------------------------------------
    // Init-flag bookkeeping. We support three orthogonal init flags:
    //   --init-flake    write a top-level flake.nix
    //   --init-npins    scaffold an npins/ directory + default.nix
    //
    // When the structured layout is active (always for
    // --init-npins; opted into for --init-flake when no PATH was
    // given), files land at:
    //   ./flake.nix           (--init-flake)
    //   ./default.nix         (--init-npins)
    //   ./npins/              (--init-npins)
    //   ./nix/overlay.nix
    //   ./nix//package.nix
    //   ./nix/modules/<pname>/default.nix   (module template only)
    // ----------------------------------------------------------------
    let init_flake = matches.is_present("init-flake");
    let init_npins = matches.is_present("init-npins");
    let no_path_given = matches.occurrences_of("PATH") == 0;

    // Decide whether to use the structured nix/ layout.
    //   - Always for --init-npins, and --init-flake when no explicit
    //     PATH was given. (--init-flake with an explicit PATH preserves
    //     the legacy flat layout for scripts that depend on it.)
    //   - Never when --by-name is in play (it has its own canonical
    //     placement under nixpkgs).
    let nixpkgs_layout_active = matches.is_present("by-name");
    let use_structured_layout =
        !nixpkgs_layout_active && (init_npins || (init_flake && no_path_given));

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
        if info.template == Template::Module {
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
    let output_content = info.format(&expr);

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
                expression::generate_flake_nix(&info.template, output_filename, directory_name),
            ))
        }
    } else {
        None
    };

    // ----- overlay.nix payload (structured layout only) -----
    let overlay_payload: Option<(std::path::PathBuf, String)> = layout.as_ref().map(|l| {
        (
            l.overlay_path.clone(),
            expression::generate_overlay_nix(&info.template, &info.pname),
        )
    });

    // ----- top-level default.nix payload (structured layout only) -----
    // Emitted whenever --init-npins is in play, so that non-flake
    // consumers have a working entry point. We skip it for
    // `--init-flake` alone since flake.nix is the only entry point
    // the user asked for in that case.
    let want_top_default = layout.is_some() && init_npins;
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

            let wrapper_content =
                expression::generate_npins_wrapper_default_nix(&info.template, &package_basename);

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

    if matches.is_present("stdout") {
        println!("{}", output_content);
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
            println!("\n# ===== {} =====\n", npins_default_path.display());
            println!("{}", npins_default_content);
            println!("\n# ===== {} =====\n", npins_sources_path.display());
            println!("{}", npins_sources_content);
            if let Some((wrapper_path, wrapper_content)) = legacy_wrapper {
                println!("\n# ===== {} =====\n", wrapper_path.display());
                println!("{}", wrapper_content);
            }
        }
    } else {
        let path = &info.path_to_write;

        // write main package file
        output::write_file(path, &output_content);
        println!(
            "Generated a {} nix expression at {}",
            &info.template,
            &output::display_path_pub(path).display()
        );

        // Write overlay.nix (structured layout only). Done before
        // flake/default so the imports referenced by those wrappers
        // exist on disk in the order a user inspecting progress
        // would expect.
        if let Some((overlay_path, overlay_content)) = &overlay_payload {
            output::write_new(overlay_path, overlay_content, "overlay.nix");
        }

        // Write top-level default.nix (structured layout only).
        if let Some((top_path, top_content)) = &top_default_payload {
            output::write_new(top_path, top_content, "top-level default.nix");
        }

        // Write flake.nix if --init-flake was provided.
        if let Some((flake_path, flake_content)) = &flake_payload {
            output::write_new(flake_path, flake_content, "flake.nix");
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

            output::write_new(
                &npins_default_path,
                &npins_default_content,
                "npins lockfile reader",
            );
            output::write_new(
                &npins_sources_path,
                &npins_sources_content,
                "empty npins/sources.json",
            );
            if let Some((wrapper_path, wrapper_content)) = legacy_wrapper {
                output::write_new(&wrapper_path, &wrapper_content, "npins wrapper default.nix");
            }

            println!();
            println!("Next steps (assuming npins v0.4.0+):");
            let project_dir = npins_dir
                .parent()
                .map(|p| {
                    let s = p.display().to_string();
                    if s.is_empty() {
                        ".".to_string()
                    } else {
                        s
                    }
                })
                .unwrap_or_else(|| ".".into());
            println!("  1. cd into {} (if not already there)", project_dir);
            println!("  2. Pin nixpkgs:  npins add channel --name nixpkgs nixpkgs-unstable");
            println!("  3. Build:        nix-build");
            println!();
        }

        // Note: --by-name packages are auto-discovered via RFC140, so no
        // manual addition to all-packages.nix is needed.
        if matches.is_present("by-name") {
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

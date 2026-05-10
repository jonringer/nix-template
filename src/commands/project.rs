use crate::{
    cli, expression,
    file_path::NixDirLayout,
    interactive, output,
    types::{Template, UserConfig},
};

pub fn run(
    matches: &clap::ArgMatches,
    xdg_dirs: &xdg::BaseDirectories,
    user_config: Option<&UserConfig>,
) {
    // Determine which project subcommand was used and cross-flags
    let (init_flake, init_npins, sub_matches) = match matches.subcommand() {
        ("flake", Some(m)) => {
            let npins = m.is_present("with-npins");
            (true, npins, m)
        }
        ("npins", Some(m)) => {
            let flake = m.is_present("with-flake");
            (flake, true, m)
        }
        _ => {
            eprintln!("Expected 'flake' or 'npins' subcommand. Run 'nix-template project --help' for usage.");
            std::process::exit(1);
        }
    };

    run_project(sub_matches, xdg_dirs, user_config, init_flake, init_npins);
}

fn run_project(
    matches: &clap::ArgMatches,
    _xdg_dirs: &xdg::BaseDirectories,
    user_config: Option<&UserConfig>,
    init_flake: bool,
    init_npins: bool,
) {
    let cwd = std::env::current_dir().unwrap_or_default();

    // Pre-detect to see if there are actual project files
    let candidates = crate::detect::detect_template_candidates_from_path(&cwd);
    let has_local_project_files = !candidates.is_empty();

    if !has_local_project_files {
        eprintln!("Warning: no project files detected in current directory.");
    }

    // Get current directory name for default pname (convert to kebab-case)
    let directory_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase().replace('_', "-").replace(' ', "-"))
        .unwrap_or_else(|| "my-project".to_owned());

    // Auto-detect template, infer dependencies, and detect builder variants
    let mut local_use_cargo_lock_file = false;
    let mut local_cargo_lock_git_deps: Vec<String> = Vec::new();
    let mut local_go_vendor_null = false;
    let mut local_go_module_path = String::new();
    let mut local_python_format: Option<String> = None;
    let mut local_python_propagated_deps: Vec<String> = Vec::new();
    let mut local_php_extensions: Option<Vec<String>> = None;
    let mut local_php_version: Option<String> = None;

    // Detect template from local files
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
            crate::types::Template::Php(_) => {
                let composer_json = cwd.join("composer.json");
                if composer_json.exists() {
                    let extensions = crate::deps::php::detect_php_extensions(&composer_json);
                    if !extensions.is_empty() {
                        eprintln!("Detected PHP extensions: {}", extensions.join(", "));
                    }
                    local_php_extensions = Some(extensions);

                    if let Some(version) = crate::deps::php::detect_php_version(&composer_json) {
                        eprintln!("Detected PHP version: {}", version);
                        local_php_version = Some(version);
                    }
                }
            }
            _ => {}
        }
    }

    // Infer dependencies for the detected template
    let mut inferred_deps = if let Some(candidate) = candidates.first() {
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
            crate::types::Template::Php(_) => {
                let composer_json = cwd.join("composer.json");
                if composer_json.exists() {
                    crate::deps::php::infer_native_dependencies(&composer_json)
                } else {
                    (Vec::new(), Vec::new())
                }
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
    if let Some((build_inputs, native_build_inputs)) =
        crate::deps::buildsystem::infer_buildsystem_dependencies_from_path(&cwd)
    {
        inferred_deps.0.extend(build_inputs);
        inferred_deps.1.extend(native_build_inputs);
    }

    // Detect if we should enter interactive mode:
    // Essential info is missing (pname, license, maintainer)
    let should_use_interactive = matches.value_of("pname") == Some("CHANGE")
        || matches.value_of("license") == Some("CHANGE")
        || matches.value_of("maintainer") == Some("CHANGE");

    let mut info = if should_use_interactive {
        let interactive_result = interactive::run_interactive_mode_with_defaults(
            None,
            user_config,
            candidates,
            Some(directory_name.clone()),
            Some(inferred_deps),
            true, // is_local_init
        );

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
        // Build info from CLI args directly
        let template: Template = cli::arg_to_type(matches.value_of("TEMPLATE"));
        let pname: String = cli::arg_to_type(matches.value_of("pname"));
        let version: String = cli::arg_to_type(matches.value_of("v"));
        let license: String = cli::arg_to_type(matches.value_of("license"));
        let include_documentation_links: bool = matches.is_present("documentation-links");
        let include_meta: bool = !matches.is_present("no-meta");

        let maintainer: String = if let Some(ref config) = user_config {
            matches
                .value_of("maintainer")
                .or_else(|| config.maintainer.as_deref())
                .unwrap_or("")
                .to_owned()
        } else {
            matches.value_of("maintainer").unwrap_or("").to_string()
        };

        let mut cli_info = crate::types::ExpressionInfo {
            pname,
            version,
            license,
            maintainer,
            template,
            fetcher: crate::types::Fetcher::local,
            path_to_write: std::path::PathBuf::new(),
            top_level_path: std::path::PathBuf::new(),
            include_documentation_links,
            include_meta,
            tag_prefix: "".to_owned(),
            owner: "CHANGE".to_owned(),
            src_sha: "0000000000000000000000000000000000000000000000000000".to_owned(),
            description: "CHANGE".to_owned(),
            homepage: "https://github.com/@owner@/@pname@".to_owned(),
            propagated_build_inputs: Vec::new(),
            cargo_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            vendor_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            npm_deps_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            pnpm_deps_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            project_file: "CHANGE".to_owned(),
            domain: "CHANGE".to_owned(),
            build_inputs: Vec::new(),
            native_build_inputs: Vec::new(),
            use_cargo_lock_file: false,
            cargo_lock_git_deps: Vec::new(),
            go_module_path: String::new(),
            python_format: "setuptools".to_owned(),
            mvn_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            mix_fod_hash: crate::types::FAKE_SRI_HASH.to_owned(),
            gradle_hash: crate::types::FAKE_SRI_HASH.to_owned(),
        };

        // Auto-detect template if not explicitly set
        if cli_info.template == crate::types::Template::Auto {
            if !candidates.is_empty() {
                cli_info.template = candidates[0].template.clone();
            } else if !matches.is_present("no-detect") {
                eprintln!("nix-template: no build system detected; defaulting to stdenv");
                cli_info.template =
                    Template::Stdenv(crate::types::StdenvVariant::Default);
            } else {
                cli_info.template =
                    Template::Stdenv(crate::types::StdenvVariant::Default);
            }
        }

        // Use directory name if pname is still CHANGE
        if cli_info.pname == "CHANGE" {
            cli_info.pname = directory_name.clone();
        }

        // Apply inferred dependencies
        if !inferred_deps.0.is_empty() || !inferred_deps.1.is_empty() {
            cli_info.build_inputs = inferred_deps.0.clone();
            cli_info.native_build_inputs = inferred_deps.1.clone();
        }

        // Merge user-supplied build inputs
        let cli_bi = cli::collect_input_args(matches, "build-inputs");
        let cli_nbi = cli::collect_input_args(matches, "native-build-inputs");
        cli_info.build_inputs = cli::merge_dedup(&cli_info.build_inputs, cli_bi);
        cli_info.native_build_inputs = cli::merge_dedup(&cli_info.native_build_inputs, cli_nbi);

        cli_info
    };

    // Apply local development builder variants
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
    if let Some(php_config) = info.template.php_config_mut() {
        if let Some(extensions) = local_php_extensions {
            php_config.extensions = extensions;
        }
        if let Some(version) = local_php_version {
            php_config.version = Some(version);
        }
    }

    // Always local fetcher for project mode
    info.fetcher = crate::types::Fetcher::local;

    // Always use structured nix/ layout for project mode
    let layout = NixDirLayout::new(
        std::path::Path::new(""),
        &info.pname,
        &info.template,
    );

    // Rewrite the package output path for the structured layout.
    if info.template == Template::Module {
        if let Some(ref module_path) = layout.module_path {
            info.path_to_write = module_path.clone();
        }
    } else {
        info.path_to_write = layout.package_path.clone();
    }

    let expr = expression::generate_expression(&info);
    let output_content = info.format(&expr);

    // Helper: directory name for flake description
    let directory_name_owned = std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.file_name().map(|n| n.to_owned()))
        .and_then(|n| n.to_str().map(|s| s.to_owned()))
        .unwrap_or_else(|| "CHANGE".to_owned());
    let directory_name_str = directory_name_owned.as_str();

    // ----- flake.nix payload -----
    let flake_payload: Option<(std::path::PathBuf, String)> = if init_flake {
        Some((
            layout.top_flake_nix.clone(),
            expression::generate_structured_flake_nix(
                &info.template,
                &info.pname,
                directory_name_str,
            ),
        ))
    } else {
        None
    };

    // ----- overlay.nix payload -----
    let overlay_payload = (
        layout.overlay_path.clone(),
        expression::generate_overlay_nix(&info.template, &info.pname),
    );

    // ----- top-level default.nix payload (when npins is in play) -----
    let top_default_payload: Option<(std::path::PathBuf, String)> = if init_npins {
        Some((
            layout.top_default_nix.clone(),
            expression::generate_structured_default_nix(
                &info.template,
                &info.pname,
                init_npins,
            ),
        ))
    } else {
        None
    };

    // ----- npins payload -----
    let npins_payload = if init_npins {
        let npins_dir = layout.npins_dir.clone();
        let npins_default_path = npins_dir.join("default.nix");
        let npins_sources_path = npins_dir.join("sources.json");
        Some((
            npins_dir,
            npins_default_path,
            expression::generate_npins_default_nix().to_string(),
            npins_sources_path,
            expression::generate_npins_sources_json().to_string(),
        ))
    } else {
        None
    };

    if matches.is_present("stdout") {
        println!("{}", output_content);
        if let Some((flake_path, flake)) = &flake_payload {
            println!("\n# ===== {} =====\n", flake_path.display());
            println!("{}", flake);
        }
        println!("\n# ===== {} =====\n", overlay_payload.0.display());
        println!("{}", overlay_payload.1);
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
        )) = &npins_payload
        {
            println!("\n# ===== {} =====\n", npins_default_path.display());
            println!("{}", npins_default_content);
            println!("\n# ===== {} =====\n", npins_sources_path.display());
            println!("{}", npins_sources_content);
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

        // Write overlay.nix
        output::write_new(&overlay_payload.0, &overlay_payload.1, "overlay.nix");

        // Write top-level default.nix
        if let Some((top_path, top_content)) = &top_default_payload {
            output::write_new(top_path, top_content, "top-level default.nix");
        }

        // Write flake.nix
        if let Some((flake_path, flake_content)) = &flake_payload {
            output::write_new(flake_path, flake_content, "flake.nix");
        }

        // Write npins scaffold
        if let Some((
            npins_dir,
            npins_default_path,
            npins_default_content,
            npins_sources_path,
            npins_sources_content,
        )) = npins_payload
        {
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
    }
}

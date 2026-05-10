use crate::{
    cli, expression, interactive, output,
    types::UserConfig,
};

pub fn run(
    matches: &clap::ArgMatches,
    _xdg_dirs: &xdg::BaseDirectories,
    user_config: Option<&UserConfig>,
) {
    // Detect if we should enter interactive mode:
    // Template was not explicitly provided AND no URL AND pname is "CHANGE"
    let should_use_interactive = matches.occurrences_of("TEMPLATE") == 0
        && matches.occurrences_of("from-url") == 0
        && matches.value_of("pname") == Some("CHANGE");

    let mut info = if should_use_interactive {
        match interactive::run_interactive_mode(None, user_config) {
            Ok(interactive_data) => {
                cli::build_expression_info_from_interactive(interactive_data, user_config)
            }
            Err(e) => {
                eprintln!("Interactive mode cancelled or failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        cli::validate_and_serialize_template_matches(matches, user_config)
    };

    // PHP detection for explicit mode (running from local dir)
    if info.template.is_php() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let composer_json = cwd.join("composer.json");
        if composer_json.exists() {
            let extensions = crate::deps::php::detect_php_extensions(&composer_json);
            if !extensions.is_empty() {
                eprintln!("Detected PHP extensions: {}", extensions.join(", "));
                if let Some(php_config) = info.template.php_config_mut() {
                    php_config.extensions = extensions;
                }
            }

            if let Some(version) = crate::deps::php::detect_php_version(&composer_json) {
                eprintln!("Detected PHP version: {}", version);
                if let Some(php_config) = info.template.php_config_mut() {
                    php_config.version = Some(version);
                }
            }
        }
    }

    let expr = expression::generate_expression(&info);
    let output_content = info.format(&expr);

    if matches.is_present("stdout") {
        println!("{}", output_content);
    } else {
        let path = &info.path_to_write;

        output::write_file(path, &output_content);
        println!(
            "Generated a {} nix expression at {}",
            &info.template,
            &output::display_path_pub(path).display()
        );

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

/// Entry point for bare `nix-template` with no subcommand (interactive mode).
pub fn run_interactive(
    _xdg_dirs: &xdg::BaseDirectories,
    user_config: Option<&UserConfig>,
) {
    let info = match interactive::run_interactive_mode(None, user_config) {
        Ok(interactive_data) => {
            cli::build_expression_info_from_interactive(interactive_data, user_config)
        }
        Err(e) => {
            eprintln!("Interactive mode cancelled or failed: {}", e);
            std::process::exit(1);
        }
    };

    let expr = expression::generate_expression(&info);
    let output_content = info.format(&expr);

    let path = &info.path_to_write;
    output::write_file(path, &output_content);
    println!(
        "Generated a {} nix expression at {}",
        &info.template,
        &output::display_path_pub(path).display()
    );
}

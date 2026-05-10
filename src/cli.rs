use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use std::io::IsTerminal;

use crate::deps::buildsystem;
use crate::deps::go::infer_go_dependencies;
use crate::deps::ruby;
use crate::deps::rust::infer_rust_dependencies;
use crate::file_path::nix_file_paths;
use crate::interactive::InteractiveData;
use crate::types::{ExpressionInfo, Fetcher, Template, UserConfig, FAKE_SRI_HASH};
use crate::url::{infer_dotnet_project_file, prefetch_dependency_hash, read_meta_from_url};

// clap will validate inputs, only use on functions with possible_values defined
pub fn arg_to_type<T>(arg: Option<&str>) -> T
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    arg.unwrap().parse::<T>().unwrap()
}

// There is the assert macro, but the panic output does not look great
pub fn assert(pred: bool, message: &str) {
    if !pred {
        eprintln!("{}", message);
        #[cfg(not(test))]
        std::process::exit(1);
        #[cfg(test)]
        panic!("{}", message)
    }
}

/// Args shared by both `template` and `project` subcommands.
fn shared_args() -> Vec<Arg<'static, 'static>> {
    vec![
        Arg::from_usage("-l,--license [license] 'Set license'").default_value("CHANGE"),
        Arg::from_usage("-m,--maintainer [maintainer] 'Set maintainer'"),
        Arg::from_usage("--no-meta 'Don't include meta section'"),
        Arg::from_usage(
            "-d,--documentation-links 'Add comments linking to relevant sections of the Nixpkgs contributor guide.'",
        )
        .takes_value(false),
        Arg::from_usage("-s,--stdout 'Write expression to stdout, instead of PATH'"),
        Arg::with_name("build-inputs")
            .long("build-inputs")
            .visible_alias("binputs")
            .takes_value(true)
            .multiple(true)
            .number_of_values(1)
            .use_delimiter(true)
            .require_delimiter(false)
            .help("Comma-separated list of nixpkgs attributes to add to buildInputs (and the function header). May be repeated. Combined with any inferred entries; duplicates are removed."),
        Arg::with_name("native-build-inputs")
            .long("native-build-inputs")
            .visible_alias("nbinputs")
            .takes_value(true)
            .multiple(true)
            .number_of_values(1)
            .use_delimiter(true)
            .require_delimiter(false)
            .help("Comma-separated list of nixpkgs attributes to add to nativeBuildInputs (and the function header). May be repeated. Combined with any inferred entries; duplicates are removed."),
        Arg::from_usage("-v [version] 'Set version of package'").default_value("0.0.1"),
        Arg::from_usage("-p,--pname [pname] 'Package name to be used in expression'")
            .default_value("CHANGE"),
        Arg::from_usage(
            "--skip-infer-deps 'Skip automatic inference of buildInputs/nativeBuildInputs.'",
        )
        .takes_value(false),
        Arg::from_usage(
            "--no-detect 'Disable automatic template detection from build system files.'",
        )
        .takes_value(false),
    ]
}

/// Args only used by the `template` subcommand.
fn template_args() -> Vec<Arg<'static, 'static>> {
    vec![
        Arg::from_usage(
            "-u,--from-url [url] 'Point to a github repo, and use github api to determine package values'",
        ),
        Arg::from_usage("-f,--fetcher [fetcher] 'Fetcher to use'")
            .possible_values(&Fetcher::variants())
            .case_insensitive(true)
            .default_value("github")
            .default_value_if("TEMPLATE", Some("python_package"), "pypi")
            .default_value_if("TEMPLATE", Some("python_application"), "pypi"),
        Arg::from_usage("-r,--nixpkgs-root [path] 'Set root of the nixpkgs directory'")
            .env("NIXPKGS_ROOT"),
        Arg::from_usage(
            "--by-name 'RFC140 layout: write the expression to pkgs/by-name/<shard>/<pname>/package.nix (relative to --nixpkgs-root).'",
        )
        .takes_value(false)
        .conflicts_with("no-meta"),
        Arg::from_usage(
            "--skip-vendor-hashes 'Skip automatic computation of cargoHash/vendorHash for rust/go templates.'",
        )
        .takes_value(false),
        Arg::from_usage(
            "--include-prereleases 'Include prerelease versions when fetching from GitLab or other forges.'",
        )
        .takes_value(false),
    ]
}

fn build_template_subcommand() -> App<'static, 'static> {
    let mut cmd = SubCommand::with_name("template")
        .about("Generate a nix expression for nixpkgs or standalone use")
        .arg(
            Arg::from_usage("<TEMPLATE> 'Language or framework template target, or a URL. Use \"auto\" to detect from source.'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("auto"),
        )
        .arg(
            Arg::from_usage("[PATH] 'Directory or file to be written.'")
                .default_value("default.nix")
                .default_value_if("TEMPLATE", Some("mkshell"), "shell.nix")
                .default_value_if("TEMPLATE", Some("test"), "test.nix"),
        );

    for arg in shared_args().into_iter().chain(template_args()) {
        cmd = cmd.arg(arg);
    }
    cmd
}

fn build_project_flake_subcommand() -> App<'static, 'static> {
    let mut cmd = SubCommand::with_name("flake")
        .about("Initialize current directory as a Nix flake project. Auto-detects project type and infers dependencies from local files.")
        .arg(
            Arg::from_usage("[TEMPLATE] 'Language or framework template target. Use \"auto\" to detect from local files.'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("auto"),
        )
        .arg(
            Arg::from_usage("--with-npins 'Also scaffold npins dependency management'")
                .takes_value(false),
        );

    for arg in shared_args() {
        cmd = cmd.arg(arg);
    }
    cmd
}

fn build_project_npins_subcommand() -> App<'static, 'static> {
    let mut cmd = SubCommand::with_name("npins")
        .about("Initialize current directory with npins dependency management. Auto-detects project type and infers dependencies from local files. See https://github.com/andir/npins")
        .arg(
            Arg::from_usage("[TEMPLATE] 'Language or framework template target. Use \"auto\" to detect from local files.'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("auto"),
        )
        .arg(
            Arg::from_usage("--with-flake 'Also generate flake.nix'")
                .takes_value(false),
        );

    for arg in shared_args() {
        cmd = cmd.arg(arg);
    }
    cmd
}

fn build_project_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("project")
        .about("Initialize current directory as a Nix project")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(build_project_flake_subcommand())
        .subcommand(build_project_npins_subcommand())
}

pub fn build_cli() -> App<'static, 'static> {
    App::new("nix-template")
        .version("0.4.1")
        .author("Jon Ringer <jonringer117@gmail.com>")
        .about("Create common nix expressions")
        .version_short("V")
        .setting(AppSettings::ColoredHelp)
        .after_help(
            "ENV VARS:

    GITHUB_TOKEN\tToken used during GitHub API calls.
    GITLAB_TOKEN\tToken used during GitLab API calls (uses PRIVATE-TOKEN header).
    GITEA_TOKEN\t\tToken used during Gitea API calls (uses Authorization header).

EXAMPLES:

# generate an expression and infer dependencies for this package
$ nix-template template rust --from-url https://github.com/jonringer/nix-template ./package.nix

# generate a template from a URL (auto-detects template type)
$ nix-template template https://pypi.org/project/requests/

# generate a boilerplate python package expression with name
$ nix-template template python_package --pname requests

# generate a shell.nix in $PWD
$ nix-template template mkshell

# initialize current directory as a flake project
$ nix-template project flake

# initialize with npins and also generate flake.nix
$ nix-template project npins --with-flake

# set maintainer name and location of nixpkgs, only needs to be set once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root ~/nixpkgs

",
        )
        .subcommand(build_template_subcommand())
        .subcommand(build_project_subcommand())
        .subcommand(
            SubCommand::with_name("completions")
                .about("Generate shell completion scripts, writes to stdout")
                .arg(
                    Arg::from_usage("<SHELL>")
                        .case_insensitive(true)
                        .possible_values(&clap::Shell::variants()),
                ),
        )
        .subcommand(
            SubCommand::with_name("config")
                .about("Set information about nix-template usage. Writes to $XDG_CONFIG_HOME")
                .arg(
                    Arg::from_usage("-f,--file [config-file] 'Config file location. [default: $XDG_CONFIG_HOME/nix-template/config.toml]'")
                )
                .subcommand(
                    SubCommand::with_name("name")
                    .about("Set maintainer name")
                    .arg(Arg::from_usage("<name>"))
                )
                .subcommand(
                    SubCommand::with_name("nixpkgs-root")
                    .about("Set the root directory of nixpkgs")
                    .arg(Arg::from_usage("<nixpkgs-root>"))
                )
        )
}

/// Pull every value supplied for an argument that allows comma-separated
/// and/or repeated values, trim whitespace around each token, and drop
/// empties. Returns an empty vec when the flag wasn't provided.
pub fn collect_input_args(matches: &ArgMatches, name: &str) -> Vec<String> {
    matches
        .values_of(name)
        .map(|vs| {
            vs.flat_map(|s| s.split(','))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Merge `extra` into `existing` and remove duplicates while preserving
/// the order of first appearance. Used to combine inferred and
/// user-supplied input lists without producing repeats.
pub fn merge_dedup(existing: &[String], extra: Vec<String>) -> Vec<String> {
    let mut combined: Vec<String> = existing.to_vec();
    combined.extend(extra);
    let mut seen = std::collections::HashSet::new();
    combined.retain(|s| seen.insert(s.clone()));
    combined
}

/// Check if the TEMPLATE positional is actually a URL.
/// Returns `true` for values starting with `http://` or `https://`.
pub fn is_url_value(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

pub fn validate_and_serialize_template_matches(
    matches: &ArgMatches,
    user_config: Option<&UserConfig>,
) -> ExpressionInfo {
    let template_str = matches.value_of("TEMPLATE").unwrap_or("auto");

    // Check if the TEMPLATE positional is actually a URL
    let (template, url_from_positional) = if is_url_value(template_str) {
        (Template::Auto, Some(template_str.to_owned()))
    } else {
        (arg_to_type(Some(template_str)), None)
    };

    let fetcher: Fetcher = arg_to_type(matches.value_of("fetcher"));
    let pname: String = arg_to_type(matches.value_of("pname"));
    let version: String = arg_to_type(matches.value_of("v"));
    let license: String = arg_to_type(matches.value_of("license"));
    let path_str: String = arg_to_type(matches.value_of("PATH"));
    let path = std::path::PathBuf::from(&path_str);
    let include_documentation_links: bool = matches.is_present("documentation-links");
    let include_meta: bool = !matches.is_present("no-meta");

    let nixpkgs_layout = matches.is_present("by-name");
    let has_url = url_from_positional.is_some() || matches.is_present("from-url");
    assert(
        !(nixpkgs_layout
            && matches.value_of("pname") == Some("CHANGE")
            && !has_url),
        "'-p,--pname' or '-u,--from-url' is required when using the --by-name flag",
    );

    if nixpkgs_layout {
        match &template {
            Template::Module | Template::Test | Template::Mkshell => {
                assert(
                    false,
                    "--by-name cannot be used with the 'module', 'test', or 'mkshell' templates",
                );
            }
            _ => {}
        }
    }

    let maintainer: String;
    let nixpkgs_root: String;
    if let Some(ref config) = user_config {
        maintainer = matches
            .value_of("maintainer")
            .or_else(|| config.maintainer.as_deref())
            .unwrap_or("")
            .to_owned();
        nixpkgs_root = matches
            .value_of("nixpkgs-root")
            .or_else(|| config.nixpkgs_root.as_deref())
            .unwrap_or("")
            .to_owned();
    } else {
        maintainer = matches.value_of("maintainer").unwrap_or("").to_string();
        nixpkgs_root = matches.value_of("nixpkgs-root").unwrap_or("").to_string();
    };

    let mut info = ExpressionInfo {
        pname,
        version,
        license,
        maintainer,
        template,
        fetcher,
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
        cargo_hash: FAKE_SRI_HASH.to_owned(),
        vendor_hash: FAKE_SRI_HASH.to_owned(),
        npm_deps_hash: FAKE_SRI_HASH.to_owned(),
        pnpm_deps_hash: FAKE_SRI_HASH.to_owned(),
        project_file: "CHANGE".to_owned(),
        domain: "CHANGE".to_owned(),
        build_inputs: Vec::new(),
        native_build_inputs: Vec::new(),
        use_cargo_lock_file: false,
        cargo_lock_git_deps: Vec::new(),
        go_module_path: String::new(),
        python_format: "setuptools".to_owned(),
        mvn_hash: FAKE_SRI_HASH.to_owned(),
        mix_fod_hash: FAKE_SRI_HASH.to_owned(),
        gradle_hash: FAKE_SRI_HASH.to_owned(),
    };

    // Handle URL: either from positional or from --from-url flag
    let url = url_from_positional
        .as_deref()
        .or_else(|| matches.value_of("from-url"));
    if let Some(url) = url {
        let include_prereleases = matches.is_present("include-prereleases");
        read_meta_from_url(url, &mut info, include_prereleases);
    }

    // Auto-detect template when "auto" is selected (either explicitly or as
    // default). Uses remote source (--from-url) or local directory (CWD).
    if info.template == Template::Auto && !matches.is_present("no-detect") {
        let candidates = if url.is_some() {
            // Remote detection: materialise source from URL
            crate::detect::detect_template_candidates(&info)
        } else {
            // Local detection: scan current working directory
            let cwd = std::env::current_dir().unwrap_or_default();
            crate::detect::detect_template_candidates_from_path(&cwd)
        };

        match candidates.len() {
            0 => {
                eprintln!("nix-template: no build system detected; defaulting to stdenv");
                info.template = Template::Stdenv(crate::types::StdenvVariant::Default);
            }
            1 => {
                eprintln!(
                    "nix-template: auto-detected template '{}' (found {})",
                    candidates[0].template, candidates[0].reason
                );
                info.template = candidates[0].template.clone();
            }
            _ => {
                if std::io::stdin().is_terminal() {
                    match crate::interactive::prompt_template_from_candidates(&candidates) {
                        Ok(chosen) => {
                            info.template = chosen;
                        }
                        Err(e) => {
                            eprintln!("Template selection cancelled: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    // Non-interactive: use highest-priority candidate
                    eprintln!(
                        "nix-template: auto-detected template '{}' (found {})",
                        candidates[0].template, candidates[0].reason
                    );
                    info.template = candidates[0].template.clone();
                }
            }
        }
    } else if info.template == Template::Auto {
        // --no-detect was specified
        info.template = Template::Stdenv(crate::types::StdenvVariant::Default);
    }

    // Python format auto-detection
    if info.template.is_python() {
        let format_str = if url.is_some() {
            if let Some(source_path) = crate::source::materialise_source(&info) {
                crate::detect::detect_python_format(&source_path)
            } else {
                "setuptools".to_owned()
            }
        } else {
            let cwd = std::env::current_dir().unwrap_or_default();
            crate::detect::detect_python_format(&cwd)
        };
        if let Some(config) = info.template.python_config_mut() {
            config.format = crate::types::PythonFormat::from_str(&format_str);
        }
        info.python_format = format_str;
    }

    // Dependency hash prefetching
    let should_prefetch_hashes =
        url.is_some() && !matches.is_present("skip-vendor-hashes");
    if should_prefetch_hashes {
        if let Some(hash) = prefetch_dependency_hash(&info) {
            match &info.template {
                Template::Rust(_) => info.cargo_hash = hash,
                Template::Go(_) => info.vendor_hash = hash,
                Template::Node(config) => match config.variant {
                    crate::types::NodeVariant::Npm => info.npm_deps_hash = hash,
                    crate::types::NodeVariant::Pnpm => info.pnpm_deps_hash = hash,
                },
                _ => {}
            }
        }
    }

    // Dependency inference
    let infer_enabled = url.is_some() && !matches.is_present("skip-infer-deps");
    if infer_enabled {
        match &info.template {
            Template::Rust(_) => {
                if let Some((build, native)) = infer_rust_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::Go(_) => {
                if let Some((build, native)) = infer_go_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::Ruby => {
                ruby::infer_dependencies(&mut info);
            }
            Template::Stdenv(_) => {
                buildsystem::infer_buildsystem_dependencies(&mut info);
            }
            Template::Dotnet => {
                if let Some(project_file) = infer_dotnet_project_file(&info) {
                    info.project_file = project_file;
                }
            }
            Template::Python(_) => {
                let deps = crate::deps::python::infer_python_dependencies(&info);
                if !deps.is_empty() {
                    info.propagated_build_inputs = deps;
                }
            }
            _ => {}
        }
    }

    // Merge user-supplied build inputs
    let cli_bi = collect_input_args(matches, "build-inputs");
    let cli_nbi = collect_input_args(matches, "native-build-inputs");
    info.build_inputs = merge_dedup(&info.build_inputs, cli_bi);
    info.native_build_inputs = merge_dedup(&info.native_build_inputs, cli_nbi);

    let (path_to_write, top_level_path) =
        nix_file_paths(nixpkgs_layout, &info.template, &path, &info.pname, &nixpkgs_root);

    info.path_to_write = path_to_write.clone();
    info.top_level_path = top_level_path.clone();

    assert(
        matches.is_present("stdout") || !path_to_write.exists(),
        &format!(
            "Cannot write to file '{}', already exists",
            path_to_write.display()
        ),
    );

    info
}

/// Build ExpressionInfo from interactive mode data
pub fn build_expression_info_from_interactive(
    data: InteractiveData,
    user_config: Option<&UserConfig>,
) -> ExpressionInfo {
    let path = std::path::PathBuf::from(&data.output_path);
    let nixpkgs_root = user_config
        .and_then(|c| c.nixpkgs_root.as_deref())
        .unwrap_or("");

    let skip_vendor_hashes = data.skip_vendor_hashes;
    let infer_deps = data.infer_deps;
    let mut info = ExpressionInfo {
        pname: data.pname.clone(),
        version: data.version,
        license: data.license,
        maintainer: data.maintainer,
        template: data.template,
        fetcher: data.fetcher,
        path_to_write: std::path::PathBuf::new(),
        top_level_path: std::path::PathBuf::new(),
        include_documentation_links: data.include_documentation_links,
        include_meta: data.include_meta,
        tag_prefix: "".to_owned(),
        owner: "CHANGE".to_owned(),
        src_sha: "0000000000000000000000000000000000000000000000000000".to_owned(),
        description: data.description,
        homepage: data.homepage,
        propagated_build_inputs: Vec::new(),
        cargo_hash: FAKE_SRI_HASH.to_owned(),
        vendor_hash: FAKE_SRI_HASH.to_owned(),
        npm_deps_hash: FAKE_SRI_HASH.to_owned(),
        pnpm_deps_hash: FAKE_SRI_HASH.to_owned(),
        project_file: "CHANGE".to_owned(),
        domain: "CHANGE".to_owned(),
        build_inputs: Vec::new(),
        native_build_inputs: Vec::new(),
        use_cargo_lock_file: false,
        cargo_lock_git_deps: Vec::new(),
        go_module_path: String::new(),
        python_format: "setuptools".to_owned(),
        mvn_hash: FAKE_SRI_HASH.to_owned(),
        mix_fod_hash: FAKE_SRI_HASH.to_owned(),
        gradle_hash: FAKE_SRI_HASH.to_owned(),
    };

    // If URL was provided, fetch metadata
    if let Some(url) = data.url {
        read_meta_from_url(&url, &mut info, data.include_prereleases);
    }

    // Vendor hash prefetching is enabled by default (opt-out via skip flag).
    // Skip for Rust when using cargoLock.lockFile (no hash needed).
    if !skip_vendor_hashes && !info.use_cargo_lock_file {
        if let Some(hash) = prefetch_dependency_hash(&info) {
            match &info.template {
                Template::Rust(_) => info.cargo_hash = hash,
                Template::Go(_) => info.vendor_hash = hash,
                _ => {}
            }
        }
    }

    // Use pre-inferred dependencies if available (from init mode), otherwise infer them
    if let Some((build, native)) = data.preinferred_deps {
        info.build_inputs = build;
        info.native_build_inputs = native;
    } else if infer_deps {
        match &info.template {
            Template::Rust(_) => {
                if let Some((build, native)) = infer_rust_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::Go(_) => {
                if let Some((build, native)) = infer_go_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::Ruby => {
                ruby::infer_dependencies(&mut info);
            }
            Template::Stdenv(_) => {
                buildsystem::infer_buildsystem_dependencies(&mut info);
            }
            Template::Dotnet => {
                if let Some(project_file) = infer_dotnet_project_file(&info) {
                    info.project_file = project_file;
                }
            }
            Template::Python(_) => {
                let deps = crate::deps::python::infer_python_dependencies(&info);
                if !deps.is_empty() {
                    info.propagated_build_inputs = deps;
                }
            }
            _ => {}
        }
    }

    // Gradle variant/DSL detection always runs (not dependent on infer_deps)
    if let Template::Gradle(_) = &info.template {
        use crate::deps::gradle;
        let cwd = std::env::current_dir().unwrap_or_default();
        let variant = gradle::detect_gradle_variant(&cwd);
        let dsl = gradle::detect_gradle_dsl(&cwd);
        let jdk_version = gradle::infer_gradle_jdk_version(&cwd);

        info.template = Template::Gradle(crate::templates::types::GradleConfig {
            variant,
            dsl,
            jdk_version: Some(jdk_version),
        });
    }

    // Set the paths - use the path directly since we collected it interactively
    info.path_to_write = path.clone();
    info.top_level_path = if !nixpkgs_root.is_empty() {
        let mut p = std::path::PathBuf::from(nixpkgs_root);
        p.push(&path);
        p
    } else {
        path
    };

    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    #[test]
    fn test_python() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "python_package",
            "-r",
            "/tmp",
            "--by-name",
            "-p",
            "requests",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.value_of("pname"), Some("requests"));
        assert_eq!(tm.value_of("TEMPLATE"), Some("python_package"));
        assert_eq!(tm.value_of("fetcher"), Some("pypi"));
        assert_eq!(tm.value_of("v"), Some("0.0.1"));
        assert_eq!(tm.value_of("license"), Some("CHANGE"));
        assert_eq!(tm.value_of("nixpkgs-root"), Some("/tmp"));
        assert_eq!(tm.is_present("stdout"), false);
        assert_eq!(tm.occurrences_of("PATH"), 0);
        assert_eq!(tm.is_present("by-name"), true);
        assert_eq!(tm.occurrences_of("from-url"), 0);
    }

    #[test]
    fn test_url() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "python_package",
            "-u",
            "https://pypi.org/project/requests/",
            "--by-name",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.is_present("stdout"), false);
        assert_eq!(tm.is_present("by-name"), true);
        assert_eq!(tm.occurrences_of("from-url"), 1);
    }

    #[test]
    fn test_mkshell() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "-s",
            "mkshell",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.is_present("stdout"), true);
        assert_eq!(tm.value_of("TEMPLATE"), Some("mkshell"));
        assert_eq!(tm.value_of("PATH"), Some("shell.nix"));
        assert_eq!(tm.value_of("pname"), Some("CHANGE"));
        assert_eq!(tm.is_present("by-name"), false);
        assert_eq!(tm.occurrences_of("from-url"), 0);
    }

    #[test]
    fn test_test() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "test",
            "-m",
            "myself",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.value_of("TEMPLATE"), Some("test"));
        assert_eq!(tm.value_of("PATH"), Some("test.nix"));
        assert_eq!(tm.value_of("maintainer"), Some("myself"));
    }

    #[test]
    fn build_inputs_flag_collects_comma_and_repeated() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "stdenv",
            "-p",
            "demo",
            "--build-inputs",
            "zlib,openssl",
            "--binputs",
            "sqlite",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        let collected = collect_input_args(tm, "build-inputs");
        assert_eq!(collected, vec!["zlib", "openssl", "sqlite"]);
    }

    #[test]
    fn native_build_inputs_flag_alias_works() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "stdenv",
            "-p",
            "demo",
            "--nbinputs",
            "pkg-config,cmake",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        let collected = collect_input_args(tm, "native-build-inputs");
        assert_eq!(collected, vec!["pkg-config", "cmake"]);
    }

    #[test]
    fn merge_dedup_preserves_first_occurrence() {
        let existing = vec!["openssl".to_owned(), "zlib".to_owned()];
        let extra = vec![
            "openssl".to_owned(), // dup of existing
            "sqlite".to_owned(),  // new
            "sqlite".to_owned(),  // intra-extra dup
        ];
        let result = merge_dedup(&existing, extra);
        assert_eq!(result, vec!["openssl", "zlib", "sqlite"]);
    }

    #[test]
    fn collect_input_args_trims_and_filters() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "stdenv",
            "-p",
            "demo",
            "--build-inputs",
            " zlib , openssl,",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        let collected = collect_input_args(tm, "build-inputs");
        assert_eq!(collected, vec!["zlib", "openssl"]);
    }

    #[test]
    fn test_fetcher() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "-f",
            "gitlab",
            "-l",
            "mit",
            "stdenv",
            "default.nix",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.value_of("license"), Some("mit"));
        assert_eq!(tm.value_of("PATH"), Some("default.nix"));
        assert_eq!(tm.occurrences_of("PATH"), 1);
        assert_eq!(tm.value_of("fetcher"), Some("gitlab"));
    }

    #[test]
    #[serial]
    fn test_nixpkgs_root_env() {
        use std::env::{remove_var, set_var};
        set_var("NIXPKGS_ROOT", "/testdir/");
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "template",
            "--by-name",
            "-p",
            "test",
        ]);
        let tm = m.subcommand_matches("template").unwrap();
        assert_eq!(tm.value_of("nixpkgs-root"), Some("/testdir/"));
        remove_var("NIXPKGS_ROOT");
    }

    #[test]
    fn test_project_flake_subcommand() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "project",
            "flake",
            "rust",
            "-p",
            "myapp",
        ]);
        let pm = m.subcommand_matches("project").unwrap();
        let fm = pm.subcommand_matches("flake").unwrap();
        assert_eq!(fm.value_of("TEMPLATE"), Some("rust"));
        assert_eq!(fm.value_of("pname"), Some("myapp"));
        assert_eq!(fm.is_present("with-npins"), false);
    }

    #[test]
    fn test_project_flake_with_npins() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "project",
            "flake",
            "--with-npins",
        ]);
        let pm = m.subcommand_matches("project").unwrap();
        let fm = pm.subcommand_matches("flake").unwrap();
        assert_eq!(fm.value_of("TEMPLATE"), Some("auto"));
        assert_eq!(fm.is_present("with-npins"), true);
    }

    #[test]
    fn test_project_npins_subcommand() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "project",
            "npins",
            "-p",
            "myapp",
        ]);
        let pm = m.subcommand_matches("project").unwrap();
        let nm = pm.subcommand_matches("npins").unwrap();
        assert_eq!(nm.value_of("TEMPLATE"), Some("auto"));
        assert_eq!(nm.value_of("pname"), Some("myapp"));
        assert_eq!(nm.is_present("with-flake"), false);
    }

    #[test]
    fn test_project_npins_with_flake() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "project",
            "npins",
            "--with-flake",
            "rust",
        ]);
        let pm = m.subcommand_matches("project").unwrap();
        let nm = pm.subcommand_matches("npins").unwrap();
        assert_eq!(nm.value_of("TEMPLATE"), Some("rust"));
        assert_eq!(nm.is_present("with-flake"), true);
    }

    #[test]
    fn test_url_as_positional() {
        assert!(is_url_value("https://github.com/foo/bar"));
        assert!(is_url_value("http://example.com"));
        assert!(!is_url_value("rust"));
        assert!(!is_url_value("auto"));
    }
}

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use crate::file_path::nix_file_paths;
use crate::interactive::InteractiveData;
use crate::go_deps::infer_go_dependencies;
use crate::rust_deps::infer_rust_dependencies;
use crate::types::{ExpressionInfo, Fetcher, Template, UserConfig, FAKE_SRI_HASH};
use crate::url::{prefetch_dependency_hash, read_meta_from_url};

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

pub fn build_cli() -> App<'static, 'static> {
    App::new("nix-template")
        .version("0.4.1")
        .author("Jon Ringer <jonringer117@gmail.com>")
        .about("Create common nix expressions")
        .version_short("V")
        .setting(AppSettings::ColoredHelp)
        // make completions and other subcommands distinct from
        // default template usage
        .setting(AppSettings::SubcommandsNegateReqs)
        // make it so that completions subcommand doesn't
        // inherit global options
        .setting(AppSettings::ArgsNegateSubcommands)
        .after_help(
            "ENV VARS:

    GITHUB_TOKEN\tToken used during GitHub API calls.
    GITLAB_TOKEN\tToken used during GitLab API calls (uses PRIVATE-TOKEN header).
    GITEA_TOKEN\t\tToken used during Gitea API calls (uses Authorization header).

EXAMPLES:

# generate an expression for this package
$ nix-template rust --from-url https://github.com/jonringer/nix-template

# generate a python package expressison at pkgs/development/python-modules/requests/default.nix
$ nix-template python --nixpkgs --pname requests

# generate a shell.nix in $PWD
$ nix-template mkshell

# set maintainer name and location of nixpkgs, only needs to be set once per user
$ nix-template config name jonringer
$ nix-template config nixpkgs-root ~/nixpkgs

",
        )
        .arg(
            Arg::from_usage("<TEMPLATE> 'Language or framework template target'")
                .possible_values(&Template::variants())
                .case_insensitive(true)
                .default_value("stdenv"),
        )
        .arg(
            Arg::from_usage("[PATH] 'directory or file to be written. In the case of a directory, a default.nix will be created. When used with --nixpkgs, it will be appended to nixpkgs-root to determine path location.'")
                .default_value("default.nix")
                .default_value_if("TEMPLATE", Some("mkshell"), "shell.nix")
                .default_value_if("TEMPLATE", Some("test"), "test.nix"),
        )
        .arg(Arg::from_usage(
            "-u,--from-url [url] 'Point to a github repo, and use github api to determine package values'",
            ))
        .arg(Arg::from_usage(
            "-l,--license [license] 'Set license'",
            ).default_value("CHANGE"))
        .arg(Arg::from_usage(
            "-m,--maintainer [maintainer] 'Set maintainer'",
            ))
        .arg(Arg::from_usage(
            "--no-meta 'Don't include meta section'",
            ).conflicts_with("nixpkgs"))
        .arg(Arg::from_usage(
            "-d,--documentation-links 'Add comments linking to relevant sections of the Nixpkgs contributor guide.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "-s,--stdout 'Write expression to stdout, instead of PATH'",
            ))
        .arg(Arg::from_usage(
            "--init-flake 'Generate a flake.nix alongside the package expression'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "--init-npins 'Generate an npins/ scaffold (npins/default.nix + empty npins/sources.json) and a wrapper default.nix at the project root. The package is placed under nix/pkgs/<pname>/package.nix and an overlay is generated at nix/overlay.nix. Combinable with --init-flake / --init-project. See https://github.com/andir/npins for the dependency manager itself.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "--init-project 'Scaffold a full project layout: top-level default.nix and nix/{overlay.nix, pkgs/<pname>/package.nix, modules/<pname>/default.nix (when -t module)}. Prompts for the template if none given. Combinable with --init-flake / --init-npins to add a flake.nix or npins/ directory.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "--skip-vendor-hashes 'Skip automatic computation of cargoHash/vendorHash for rust/go templates. By default, when --from-url is provided, nix-template runs nix-build with a fake hash to compute the real hash. Requires nix to be installed.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "--include-prereleases 'Include prerelease versions when fetching from GitLab or other forges. By default, nix-template filters out versions with -alpha, -beta, -rc, etc.'",
            ).takes_value(false))
        .arg(Arg::from_usage(
            "--skip-infer-deps 'Skip automatic inference of buildInputs/nativeBuildInputs. By default, when --from-url is provided, nix-template materialises the source: for the rust template it parses Cargo.toml/Cargo.lock to detect well-known *-sys crates; for the go template it scans *.go files for `// #cgo` directives to detect pkg-config tokens and -l libraries.'",
            ).takes_value(false))
        .arg(
            // User-supplied buildInputs. Accepts comma-separated values
            // and may be repeated, e.g. `--build-inputs zlib,openssl
            // --binputs sqlite`. Merged with anything inference produced
            // and deduped before rendering.
            Arg::with_name("build-inputs")
                .long("build-inputs")
                .visible_alias("binputs")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
                .use_delimiter(true)
                .require_delimiter(false)
                .help("Comma-separated list of nixpkgs attributes to add to buildInputs (and the function header). May be repeated. Combined with any inferred entries; duplicates are removed."),
        )
        .arg(
            Arg::with_name("native-build-inputs")
                .long("native-build-inputs")
                .visible_alias("nbinputs")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
                .use_delimiter(true)
                .require_delimiter(false)
                .help("Comma-separated list of nixpkgs attributes to add to nativeBuildInputs (and the function header). May be repeated. Combined with any inferred entries; duplicates are removed."),
        )
        .arg(Arg::from_usage(
            "-v [version] 'Set version of package'",
            ).default_value("0.0.1"))
        .arg(Arg::from_usage(
            "-p,--pname [pname] 'Package name to be used in expression'",
            ).default_value("CHANGE"))
        .arg(Arg::from_usage(
            "-r,--nixpkgs-root [path] 'Set root of the nixpkgs directory'",
            ).env("NIXPKGS_ROOT"))
        .arg(Arg::from_usage(
            "-n,--nixpkgs 'Intended be used within nixpkgs, will append pname to file path, and print addition statement'",
        ).takes_value(false))
        .arg(Arg::from_usage(
            "--by-name 'RFC140 layout: write the expression to pkgs/by-name/<shard>/<pname>/package.nix (relative to --nixpkgs-root). The shard is the lowercased first two characters of pname. Implies --nixpkgs and skips printing an all-packages.nix addition line, since by-name packages are auto-discovered.'",
        ).takes_value(false))
        .arg(
            Arg::from_usage("-f,--fetcher [fetcher] 'Fetcher to use'")
                .possible_values(&Fetcher::variants())
                .case_insensitive(true)
                .default_value("github")
                .default_value_if("TEMPLATE", Some("python"), "pypi")
                .default_value_if("TEMPLATE", Some("python-package"), "pypi")
                .default_value_if("TEMPLATE", Some("python-application"), "pypi"),
        )
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
fn collect_input_args(matches: &ArgMatches, name: &str) -> Vec<String> {
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
fn merge_dedup(existing: &[String], extra: Vec<String>) -> Vec<String> {
    let mut combined: Vec<String> = existing.to_vec();
    combined.extend(extra);
    let mut seen = std::collections::HashSet::new();
    combined.retain(|s| seen.insert(s.clone()));
    combined
}

pub fn validate_and_serialize_matches(
    matches: &ArgMatches,
    user_config: Option<&UserConfig>,
) -> ExpressionInfo {
    let template: Template = arg_to_type(matches.value_of("TEMPLATE"));
    let fetcher: Fetcher = arg_to_type(matches.value_of("fetcher"));
    let pname: String = arg_to_type(matches.value_of("pname"));
    let version: String = arg_to_type(matches.value_of("v"));
    let license: String = arg_to_type(matches.value_of("license"));
    let path_str: String = arg_to_type(matches.value_of("PATH"));
    let path = std::path::PathBuf::from(&path_str);
    let include_documentation_links: bool = matches.is_present("documentation-links");
    let include_meta: bool = !matches.is_present("no-meta");

    let nixpkgs_layout = matches.is_present("nixpkgs") || matches.is_present("by-name");
    assert(!(nixpkgs_layout && matches.value_of("pname") == Some("CHANGE") && matches.value_of("from-url") == None),
        "'-p,--pname' or '-u,--from-url' is required when using the -n,--nixpkgs or --by-name flag");

    if matches.is_present("by-name") {
        match arg_to_type::<Template>(matches.value_of("TEMPLATE")) {
            Template::module | Template::test | Template::mkshell => {
                assert(false, "--by-name cannot be used with the 'module', 'test', or 'mkshell' templates");
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
        domain: "CHANGE".to_owned(),
        build_inputs: Vec::new(),
        native_build_inputs: Vec::new(),
    };

    if let Some(url) = matches.value_of("from-url") {
        let include_prereleases = matches.is_present("include-prereleases");
        read_meta_from_url(url, &mut info, include_prereleases);
    }

    // Vendor hash prefetching is on by default when --from-url is provided.
    // Users can disable via --skip-vendor-hashes.
    let should_prefetch_hashes = matches.is_present("from-url")
        && !matches.is_present("skip-vendor-hashes");
    if should_prefetch_hashes {
        if let Some(hash) = prefetch_dependency_hash(&info) {
            match info.template {
                Template::rust => info.cargo_hash = hash,
                Template::go => info.vendor_hash = hash,
                _ => {}
            }
        }
    }

    // Inference is on by default for the rust and go templates whenever we
    // have a real source to inspect. Users can disable via `--skip-infer-deps`.
    let infer_enabled = matches.is_present("from-url")
        && !matches.is_present("skip-infer-deps");
    if infer_enabled {
        match info.template {
            Template::rust => {
                if let Some((build, native)) = infer_rust_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::go => {
                if let Some((build, native)) = infer_go_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            _ => {}
        }
    }

    // Merge any user-supplied `--build-inputs` / `--native-build-inputs`
    // (alias `--binputs` / `--nbinputs`) into the lists. Inferred entries
    // come first to preserve their order; user entries are appended and
    // duplicates are stripped.
    let cli_bi = collect_input_args(matches, "build-inputs");
    let cli_nbi = collect_input_args(matches, "native-build-inputs");
    info.build_inputs = merge_dedup(&info.build_inputs, cli_bi);
    info.native_build_inputs = merge_dedup(&info.native_build_inputs, cli_nbi);

    let (path_to_write, top_level_path) =
        nix_file_paths(&matches, &info.template, &path, &info.pname, &nixpkgs_root);

    info.path_to_write = path_to_write.clone();
    info.top_level_path = top_level_path.clone();

    // The path may be rewritten downstream when one of the --init-* flags
    // triggers the structured nix/ layout. Skip the existence check in
    // that case; main.rs re-checks each artefact before writing.
    let init_will_rewrite_path = matches.is_present("init-flake")
        || matches.is_present("init-npins")
        || matches.is_present("init-project");
    assert(
        matches.is_present("stdout")
            || init_will_rewrite_path
            || !path_to_write.exists(),
        &format!("Cannot write to file '{}', already exists", path_to_write.display()),
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
        domain: "CHANGE".to_owned(),
        build_inputs: Vec::new(),
        native_build_inputs: Vec::new(),
    };

    // If URL was provided, fetch metadata
    if let Some(url) = data.url {
        read_meta_from_url(&url, &mut info, data.include_prereleases);
    }

    // Vendor hash prefetching is enabled by default (opt-out via skip flag)
    if !skip_vendor_hashes {
        if let Some(hash) = prefetch_dependency_hash(&info) {
            match info.template {
                Template::rust => info.cargo_hash = hash,
                Template::go => info.vendor_hash = hash,
                _ => {}
            }
        }
    }

    if infer_deps {
        match info.template {
            Template::rust => {
                if let Some((build, native)) = infer_rust_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            Template::go => {
                if let Some((build, native)) = infer_go_dependencies(&info) {
                    info.build_inputs = build;
                    info.native_build_inputs = native;
                }
            }
            _ => {}
        }
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
            "python",
            "-r",
            "/tmp",
            "-n",
            "-p",
            "requests",
        ]);
        println!("{:?}", m);
        assert_eq!(m.value_of("pname"), Some("requests"));
        assert_eq!(m.value_of("TEMPLATE"), Some("python"));
        assert_eq!(m.value_of("fetcher"), Some("pypi"));
        assert_eq!(m.value_of("v"), Some("0.0.1"));
        assert_eq!(m.value_of("pname"), Some("requests"));
        assert_eq!(m.value_of("license"), Some("CHANGE"));
        assert_eq!(m.value_of("nixpkgs-root"), Some("/tmp"));
        assert_eq!(m.is_present("stdout"), false);
        assert_eq!(m.occurrences_of("PATH"), 0);
        assert_eq!(m.is_present("nixpkgs"), true);
        assert!(m.occurrences_of("nixpkgs") >= 1);
        assert_eq!(m.occurrences_of("from-url"), 0);
    }

    #[test]
    fn test_url() {
        let m = build_cli().get_matches_from(vec!["nix-template", "python", "-u", "https://pypi.org/project/requests/", "-n"]);
        assert_eq!(m.is_present("stdout"), false);
        assert_eq!(m.is_present("nixpkgs"), true);
        assert_eq!(m.occurrences_of("from-url"), 1);
    }

    #[test]
    fn test_mkshell() {
        let m = build_cli().get_matches_from(vec!["nix-template", "-s", "mkshell"]);
        assert_eq!(m.is_present("stdout"), true);
        assert_eq!(m.value_of("TEMPLATE"), Some("mkshell"));
        assert_eq!(m.value_of("PATH"), Some("shell.nix"));
        assert_eq!(m.value_of("pname"), Some("CHANGE"));
        assert_eq!(m.is_present("nixpkgs"), false);
        assert_eq!(m.occurrences_of("from-url"), 0);
    }

    #[test]
    fn test_test() {
        let m = build_cli().get_matches_from(vec!["nix-template", "test", "-m", "myself"]);
        assert_eq!(m.value_of("TEMPLATE"), Some("test"));
        assert_eq!(m.value_of("PATH"), Some("test.nix"));
        assert_eq!(m.value_of("maintainer"), Some("myself"));
    }

    #[test]
    fn build_inputs_flag_collects_comma_and_repeated() {
        // Mix repeated `--build-inputs` flags with comma-separated values
        // and the short `--binputs` alias; we should get a flat list.
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "-p",
            "demo",
            "--build-inputs",
            "zlib,openssl",
            "--binputs",
            "sqlite",
        ]);
        let collected = collect_input_args(&m, "build-inputs");
        assert_eq!(collected, vec!["zlib", "openssl", "sqlite"]);
    }

    #[test]
    fn native_build_inputs_flag_alias_works() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "-p",
            "demo",
            "--nbinputs",
            "pkg-config,cmake",
        ]);
        let collected = collect_input_args(&m, "native-build-inputs");
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
        // Whitespace around tokens and an empty trailing token must be
        // tolerated.
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "stdenv",
            "-p",
            "demo",
            "--build-inputs",
            " zlib , openssl,",
        ]);
        let collected = collect_input_args(&m, "build-inputs");
        assert_eq!(collected, vec!["zlib", "openssl"]);
    }

    #[test]
    fn test_fetcher() {
        let m = build_cli().get_matches_from(vec![
            "nix-template",
            "-f",
            "gitlab",
            "-l",
            "mit",
            "stdenv",
            "default.nix",
        ]);
        assert_eq!(m.value_of("license"), Some("mit"));
        assert_eq!(m.value_of("PATH"), Some("default.nix"));
        assert_eq!(m.occurrences_of("PATH"), 1);
        assert_eq!(m.value_of("fetcher"), Some("gitlab"));
    }

    #[test]
    #[serial] // touching global env, ensure serial runs
    fn test_nixpkgs() {
        use std::env::{remove_var, set_var};
        set_var("NIXPKGS_ROOT", "/testdir/");
        let m = build_cli().get_matches_from(vec!["nix-template", "-n"]);
        assert_eq!(m.value_of("nixpkgs-root"), Some("/testdir/"));
        remove_var("NIXPKGS_ROOT");
    }
}

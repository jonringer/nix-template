#[macro_use]
extern crate lazy_static;

mod cli;
mod expression;
mod file_path;
mod types;

use cli::arg_to_type;

fn main() {
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
        _ => {
            // build expression
            let info = cli::validate_and_serialize_matches(&m);

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
                            .expect(&format!("Was unable to create directory {}", p.display()));
                    }
                }
                std::fs::write(path, expr)
                    .expect(&format!("Was unable to write to file: {}", &path.display()));
            }
        }
    }
}

mod cli;
mod expression;
mod types;

// clap will validate inputs, only use on functions with possible_values defined
fn arg_to_type<T>(arg: Option<&str>) -> T
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    arg.unwrap().parse::<T>().unwrap()
}

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
            let template: types::Template = arg_to_type(m.value_of("TEMPLATE"));
            let fetcher: types::Fetcher = arg_to_type(m.value_of("fetcher"));
            let pname: String = arg_to_type(m.value_of("pname"));

            let expr = expression::generate_expression(&template, &fetcher, &pname);
            println!("{}", expr);
        }
    }
}

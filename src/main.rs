mod cli;
mod types;

fn main() {
    let m = cli::build_cli().get_matches();
}

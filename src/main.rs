//! Command-line entry point for `retent`.

use clap::Parser;

fn main() {
    let cli = retent::cli::Cli::parse();
    if let Err(error) = retent::cli::run(cli) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

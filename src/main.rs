mod cli;
mod discover;
mod frontmatter;
mod priority;

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(cli::Outcome::Success) => ExitCode::SUCCESS,
        Ok(cli::Outcome::Invalid) => ExitCode::FAILURE,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}

use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};

use crate::frontmatter::{Column, Document};

/// Markdown-native incremental learning.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List Markdown files beneath the current directory.
    List(ListArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Print absolute paths instead of paths relative to the current directory.
    #[arg(long)]
    absolute_path: bool,

    /// Append a frontmatter column to each row.
    #[arg(long = "col", value_enum, value_name = "COLUMN")]
    columns: Vec<Column>,
}

pub(crate) enum Outcome {
    Success,
    Invalid,
}

pub(crate) fn run() -> Result<Outcome, String> {
    let cli = Cli::parse();

    match cli.command {
        Command::List(args) => list(args),
    }
}

fn list(args: ListArgs) -> Result<Outcome, String> {
    let root = std::env::current_dir()
        .map_err(|error| format!("cannot determine the current directory: {error}"))?;
    let paths = crate::discover::markdown_files(&root)?;
    let today = jiff::Zoned::now().date();
    let mut output = String::new();

    for path in paths {
        let document = Document::read(&path, today);
        let Some(values) = document.values(&args.columns) else {
            continue;
        };

        if args.absolute_path {
            output.push_str(&path.display().to_string());
        } else {
            let relative = path.strip_prefix(&root).map_err(|error| {
                format!(
                    "{} is not beneath {}: {error}",
                    path.display(),
                    root.display()
                )
            })?;
            output.push_str("./");
            output.push_str(&relative.display().to_string());
        }

        for value in values {
            output.push(' ');
            output.push_str(&value);
        }
        output.push('\n');
    }

    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| format!("cannot write output: {error}"))?;

    Ok(Outcome::Success)
}

use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};

use crate::frontmatter::{Column, Frontmatter};

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
    let today = args
        .columns
        .iter()
        .any(|column| column.needs_card_memory())
        .then(|| jiff::Zoned::now().date());
    let mut output = String::new();
    let mut invalid = false;

    for path in paths {
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

        if !args.columns.is_empty() {
            let frontmatter = Frontmatter::read(&path);
            for value in frontmatter.values(&args.columns, today) {
                invalid |= value == "?";
                output.push(' ');
                output.push_str(&value);
            }
        }
        output.push('\n');
    }

    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| format!("cannot write output: {error}"))?;

    if invalid {
        Ok(Outcome::Invalid)
    } else {
        Ok(Outcome::Success)
    }
}

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::Path;

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

    /// Check every managed Markdown file for validation errors.
    Audit(AuditArgs),

    /// Change priority frontmatter in files read from standard input.
    Priority(PriorityArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[command(flatten)]
    paths: PathArgs,

    /// Append a frontmatter column to each row.
    #[arg(long = "col", value_enum, value_name = "COLUMN")]
    columns: Vec<Column>,
}

#[derive(Debug, Args)]
struct AuditArgs {
    #[command(flatten)]
    paths: PathArgs,
}

#[derive(Debug, Args)]
struct PriorityArgs {
    #[command(subcommand)]
    operation: PriorityOperation,
}

#[derive(Debug, Subcommand)]
enum PriorityOperation {
    /// Increase an existing priority.
    Increment {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=10))]
        amount: u8,
    },

    /// Decrease an existing priority.
    Decrement {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=10))]
        amount: u8,
    },

    /// Add priority when it does not already exist.
    Add {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=10))]
        value: u8,
    },

    /// Add or replace priority.
    Upsert {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=10))]
        value: u8,
    },
}

#[derive(Debug, Args)]
struct PathArgs {
    /// Print absolute paths instead of paths relative to the current directory.
    #[arg(long)]
    absolute_path: bool,
}

pub(crate) enum Outcome {
    Success,
    Invalid,
}

pub(crate) fn run() -> Result<Outcome, String> {
    let cli = Cli::parse();

    match cli.command {
        Command::List(args) => list(args),
        Command::Audit(args) => audit(args),
        Command::Priority(args) => edit_priority(args),
    }
}

fn edit_priority(args: PriorityArgs) -> Result<Outcome, String> {
    let root = std::env::current_dir()
        .map_err(|error| format!("cannot determine the current directory: {error}"))?;
    let mut input = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut input)
        .map_err(|error| format!("cannot read standard input: {error}"))?;
    let action = match args.operation {
        PriorityOperation::Increment { amount } => crate::priority::Action::Increment(amount),
        PriorityOperation::Decrement { amount } => crate::priority::Action::Decrement(amount),
        PriorityOperation::Add { value } => crate::priority::Action::Add(value),
        PriorityOperation::Upsert { value } => crate::priority::Action::Upsert(value),
    };
    let mut edited = String::new();
    let mut skipped = String::new();
    let mut provided = HashSet::new();

    for supplied in input.lines().filter(|line| !line.is_empty()) {
        let supplied_path = Path::new(supplied);
        let path = if supplied_path.is_absolute() {
            supplied_path.to_path_buf()
        } else {
            root.join(supplied_path)
        };

        let path = match crate::priority::canonical_target(&path) {
            Ok(path) if provided.insert(path.clone()) => path,
            Ok(_) => {
                append_escaped_path(&mut skipped, supplied_path);
                skipped.push_str("\tfile was already provided\n");
                continue;
            }
            Err(reason) => {
                append_escaped_path(&mut skipped, supplied_path);
                skipped.push('\t');
                append_escaped_text(&mut skipped, &reason);
                skipped.push('\n');
                continue;
            }
        };

        match crate::priority::edit(&path, action) {
            Ok(()) => {
                append_escaped_path(&mut edited, supplied_path);
                edited.push('\n');
            }
            Err(reason) => {
                append_escaped_path(&mut skipped, supplied_path);
                skipped.push('\t');
                append_escaped_text(&mut skipped, &reason);
                skipped.push('\n');
            }
        }
    }

    edited.push_str(&skipped);
    io::stdout()
        .lock()
        .write_all(edited.as_bytes())
        .map_err(|error| format!("cannot write output: {error}"))?;

    Ok(Outcome::Success)
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

        append_path(&mut output, &root, &path, args.paths.absolute_path)?;

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

fn audit(args: AuditArgs) -> Result<Outcome, String> {
    let root = std::env::current_dir()
        .map_err(|error| format!("cannot determine the current directory: {error}"))?;
    let paths = crate::discover::markdown_files(&root)?;
    let today = jiff::Zoned::now().date();
    let mut output = String::new();
    let mut invalid = false;

    for path in paths {
        let document = Document::read(&path, today);
        let Some(issues) = document.issues() else {
            continue;
        };

        invalid = true;
        append_path(&mut output, &root, &path, args.paths.absolute_path)?;
        output.push('\t');
        for (index, issue) in issues.iter().enumerate() {
            if index > 0 {
                output.push_str("; ");
            }
            append_escaped_text(&mut output, issue.message());
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

fn append_path(
    output: &mut String,
    root: &Path,
    path: &Path,
    absolute: bool,
) -> Result<(), String> {
    if absolute {
        append_escaped_path(output, path);
    } else {
        let relative = path.strip_prefix(root).map_err(|error| {
            format!(
                "{} is not beneath {}: {error}",
                path.display(),
                root.display()
            )
        })?;
        output.push_str("./");
        append_escaped_path(output, relative);
    }
    Ok(())
}

fn append_escaped_path(output: &mut String, path: &Path) {
    if let Some(path) = path.to_str() {
        append_escaped_text(output, path);
        return;
    }

    for byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'\\' => output.push_str("\\\\"),
            b'\t' => output.push_str("\\t"),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            0x20..=0x7e => output.push(char::from(*byte)),
            _ => append_hex_escape(output, *byte),
        }
    }
}

fn append_escaped_text(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            character if character.is_control() => output.extend(character.escape_default()),
            character => output.push(character),
        }
    }
}

fn append_hex_escape(output: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    output.push_str("\\x");
    output.push(char::from(HEX[usize::from(byte >> 4)]));
    output.push(char::from(HEX[usize::from(byte & 0x0f)]));
}

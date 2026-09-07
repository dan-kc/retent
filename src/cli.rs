use std::io::{self, Write};
use std::path::Path;

use clap::{Args, Parser, Subcommand};

use crate::frontmatter::{Column, Document, Metadata};

/// Markdown-native incremental learning.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List Markdown files as NUL-terminated records.
    List(ListArgs),

    /// Check every managed Markdown file for validation errors.
    Audit(AuditArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[command(flatten)]
    paths: PathArgs,

    /// Prepend a metadata field to each record.
    #[arg(long = "col", value_enum, value_name = "COLUMN")]
    columns: Vec<Column>,
}

#[derive(Debug, Args)]
struct AuditArgs {
    #[command(flatten)]
    paths: PathArgs,
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
    }
}

fn list(args: ListArgs) -> Result<Outcome, String> {
    let root = std::env::current_dir()
        .map_err(|error| format!("cannot determine the current directory: {error}"))?;
    let files = crate::discover::vault_files(&root)?;
    let metadata = Metadata::load(files.inheritance_root, files.metadata)?;
    let today = jiff::Zoned::now().date();
    let mut output = Vec::new();

    for path in files.markdown {
        let defaults = metadata.values_for(&path);
        let document = Document::read(&path, today, &defaults);
        let Some(values) = document.values(&args.columns) else {
            continue;
        };

        for value in values {
            output.extend_from_slice(value.as_bytes());
            output.push(b'\t');
        }
        append_raw_path(&mut output, &root, &path, args.paths.absolute_path)?;
        output.push(0);
    }

    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&output)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("cannot write output: {error}"))?;

    Ok(Outcome::Success)
}

fn append_raw_path(
    output: &mut Vec<u8>,
    root: &Path,
    path: &Path,
    absolute: bool,
) -> Result<(), String> {
    if absolute {
        output.extend_from_slice(path.as_os_str().as_encoded_bytes());
    } else {
        let relative = path.strip_prefix(root).map_err(|error| {
            format!(
                "{} is not beneath {}: {error}",
                path.display(),
                root.display()
            )
        })?;
        output.extend_from_slice(b"./");
        output.extend_from_slice(relative.as_os_str().as_encoded_bytes());
    }
    Ok(())
}

fn audit(args: AuditArgs) -> Result<Outcome, String> {
    let root = std::env::current_dir()
        .map_err(|error| format!("cannot determine the current directory: {error}"))?;
    let files = crate::discover::vault_files(&root)?;
    let metadata = Metadata::load(files.inheritance_root, files.metadata)?;
    let today = jiff::Zoned::now().date();
    let mut findings: Vec<_> = metadata
        .findings()
        .iter()
        .map(|finding| {
            (
                finding.path().to_path_buf(),
                vec![finding.reason().to_owned()],
            )
        })
        .collect();

    for path in files.markdown {
        let defaults = metadata.values_for(&path);
        let document = Document::read(&path, today, &defaults);
        let Some(issues) = document.issues() else {
            continue;
        };
        findings.push((
            path,
            issues
                .iter()
                .map(|issue| issue.message().to_owned())
                .collect(),
        ));
    }

    findings.sort_by(|(left, _), (right, _)| left.as_os_str().cmp(right.as_os_str()));
    let mut output = String::new();
    for (path, issues) in &findings {
        append_path(&mut output, &root, path, args.paths.absolute_path)?;
        output.push('\t');
        for (index, issue) in issues.iter().enumerate() {
            if index > 0 {
                output.push_str("; ");
            }
            append_escaped_text(&mut output, issue);
        }
        output.push('\n');
    }

    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| format!("cannot write output: {error}"))?;

    if findings.is_empty() {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Invalid)
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
    } else if let Ok(relative) = path.strip_prefix(root) {
        output.push_str("./");
        append_escaped_path(output, relative);
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| format!("{} does not have a parent directory", path.display()))?;
        let descent = root.strip_prefix(parent).map_err(|error| {
            format!(
                "{} is neither beneath nor an ancestor of {}: {error}",
                path.display(),
                root.display()
            )
        })?;
        for _ in descent.components() {
            output.push_str("../");
        }
        let name = path
            .file_name()
            .ok_or_else(|| format!("{} does not have a file name", path.display()))?;
        append_escaped_path(output, Path::new(name));
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

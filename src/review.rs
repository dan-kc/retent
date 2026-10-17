use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap::Args;

use crate::cli::Outcome;
use crate::frontmatter::{Document, ManagedDocument, Metadata};

#[derive(Debug, Args)]
pub(crate) struct ReviewArgs {
    /// Markdown file to review.
    file: PathBuf,

    /// Card rating: 1=Again, 2=Hard, 3=Good, 4=Easy.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=4))]
    rating: Option<u32>,

    /// Last line read in a note (non-negative).
    #[arg(long, requires = "pass", conflicts_with = "rating")]
    end_line: Option<u64>,

    /// Note reading pass (non-negative and no lower than the previous pass).
    #[arg(long, requires = "end_line", conflicts_with = "rating")]
    pass: Option<u64>,
}

pub(crate) fn run(args: ReviewArgs) -> Result<Outcome, String> {
    let cwd = std::env::current_dir()
        .map_err(|error| format!("cannot determine current directory: {error}"))?;
    let input = cwd.join(&args.file);
    let info = fs::symlink_metadata(&input)
        .map_err(|error| format!("cannot inspect {}: {error}", input.display()))?;
    if !info.is_file() {
        return Err("review requires a regular file, not a directory or symlink".to_owned());
    }
    if !input
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err("review requires a Markdown (.md) file".to_owned());
    }
    let path = fs::canonicalize(&input)
        .map_err(|error| format!("cannot resolve {}: {error}", input.display()))?;
    let files = crate::discover::metadata_for_file(&cwd, &path)?;
    let metadata = Metadata::load(files.inheritance_root, files.metadata)?;
    let defaults = metadata.values_for(&path);
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let today = jiff::Zoned::now().date();
    let document = validated(&source, today, &defaults)?;
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (header, row) = match document {
        ManagedDocument::Card { .. } => {
            let rating = args.rating.ok_or("card reviews require --rating (1..4)")?;
            (
                "| Date       | Rating |\n| ---------- | -----: |",
                format!("| {today} | {rating:>6} |"),
            )
        }
        ManagedDocument::Note { .. } => {
            if args.rating.is_some() {
                return Err("note reviews require --end-line and --pass, not --rating".to_owned());
            }
            let end_line = args
                .end_line
                .ok_or("note reviews require --end-line and --pass")?;
            let pass = args
                .pass
                .ok_or("note reviews require --end-line and --pass")?;
            (
                "| Date       | End Line | Pass |\n| ---------- | -------: | ---- |",
                format!("| {today} | {end_line:>8} | {pass:<4} |"),
            )
        }
    };
    let updated = append_entry(&source, header, &row, newline);
    validated(&updated, today, &defaults)?;
    if info.permissions().readonly() {
        return Err(format!("{} is read-only", path.display()));
    }
    let parent = path.parent().ok_or("file has no parent directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create temporary file: {error}"))?;
    temporary
        .as_file()
        .set_permissions(info.permissions())
        .map_err(|error| format!("cannot preserve file permissions: {error}"))?;
    temporary
        .write_all(updated.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("cannot write review: {error}"))?;
    let current =
        fs::read(&path).map_err(|error| format!("cannot re-read {}: {error}", path.display()))?;
    if current != source.as_bytes() {
        return Err("file changed during review; no review was written".to_owned());
    }
    temporary
        .persist(&path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))?;
    Ok(Outcome::Success)
}

fn validated(
    source: &str,
    today: jiff::civil::Date,
    defaults: &serde_yaml_ng::Mapping,
) -> Result<ManagedDocument, String> {
    match Document::parse(source, today, defaults) {
        Document::Managed(document) => Ok(document),
        Document::Unmanaged => Err("file is not a managed card or note".to_owned()),
        Document::Invalid(issues) => Err(issues
            .iter()
            .map(|issue| issue.message())
            .collect::<Vec<_>>()
            .join("; ")),
    }
}

fn append_entry(source: &str, header: &str, row: &str, newline: &str) -> String {
    let mut offset = 0;
    let mut in_history = false;
    let mut insertion = None;
    let mut lines = source.split_inclusive('\n').peekable();
    if lines.peek().is_some_and(|line| {
        line.trim_start_matches('\u{feff}')
            .trim_end_matches(['\r', '\n'])
            == "---"
    }) {
        if let Some(first) = lines.next() {
            offset += first.len();
        }
        for line in lines.by_ref() {
            offset += line.len();
            if line.trim_end_matches(['\r', '\n']) == "---" {
                break;
            }
        }
    }
    for line in lines {
        match line.trim() {
            "<!-- HISTORY:BEGIN -->" => in_history = true,
            "<!-- HISTORY:END -->" if in_history => break,
            text if in_history && !text.is_empty() => insertion = Some(offset + line.len()),
            _ => {}
        }
        offset += line.len();
    }
    if let Some(position) = insertion {
        let mut updated = source.to_owned();
        updated.insert_str(position, &format!("{row}{newline}"));
        updated
    } else {
        let separator = if source.is_empty() || source.ends_with('\n') {
            ""
        } else {
            newline
        };
        let header = header.replace('\n', newline);
        format!(
            "{source}{separator}{newline}<!-- HISTORY:BEGIN -->{newline}{newline}{header}{newline}{row}{newline}{newline}<!-- HISTORY:END -->{newline}"
        )
    }
}

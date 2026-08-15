//! Command-line structure and orchestration.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::clock::{Clock, SystemClock};
use crate::discover::{markdown_files, relative};
use crate::document::{Classification, History, parse};
use crate::filter::Filter;
use crate::scheduling::card::{CardSchedulerConfig, schedule as schedule_card};
use crate::scheduling::queue::{QueueOptions, build};

/// Markdown-native incremental learning.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Audit Markdown classification.
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Format a frontmatter sequence in paths read from standard input.
    FormatList {
        /// Frontmatter field containing the sequence.
        field: String,
        /// Sequence syntax to produce.
        #[arg(long, value_enum)]
        style: ListStyle,
        /// Resolve input paths relative to this directory.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Import cards from another application.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Record a note cursor position.
    Position {
        file: PathBuf,
        end_line: u32,
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// Record a card rating from 1 (Again) to 4 (Easy).
    Rate {
        file: PathBuf,
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        rating: u8,
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// Print the unified learning queue.
    Queue,
    /// Print the first unified queue item.
    Next(NextArgs),
    /// List all scheduled entries, optionally matching a metadata filter.
    List(ListArgs),
    /// Update metadata on paths read from a file or standard input.
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum UpdateCommand {
    /// Set priority on every selected entry.
    Priority {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        priority: u8,
        #[command(flatten)]
        selection: UpdateSelection,
    },
    /// Update tags on every selected entry.
    Tags {
        #[command(subcommand)]
        command: TagUpdateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TagUpdateCommand {
    /// Add tags, either retaining or replacing the existing tags.
    Add {
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        /// How to handle tags already present on a selected entry.
        #[arg(long, value_enum, default_value_t = ExistingTags::Keep)]
        existing: ExistingTags,
        #[command(flatten)]
        selection: UpdateSelection,
    },
    /// Rename a tag, removing duplicates if the destination already exists.
    Rename {
        from: String,
        to: String,
        #[command(flatten)]
        selection: UpdateSelection,
    },
    /// Remove one or more tags.
    Remove {
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[command(flatten)]
        selection: UpdateSelection,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExistingTags {
    Keep,
    Overwrite,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ListStyle {
    Flow,
    Block,
    Toggle,
}

#[derive(Debug, Args)]
struct UpdateSelection {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Read newline-delimited paths from this file, or `-` for standard input.
    #[arg(long)]
    files_from: PathBuf,
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// Create or resume a Markdown vault from an Anki collection package.
    Anki {
        /// An Anki .colpkg export containing scheduling information.
        file: PathBuf,
        /// Destination vault (defaults to the archive name without .colpkg).
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    /// Show documents missing scheduler fields or card sections.
    Missing(RootArgs),
    /// Show documents with syntax or semantic errors.
    Invalid(RootArgs),
}

#[derive(Debug, Args)]
struct RootArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Args)]
struct NextArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, value_parser = parse_date)]
    as_of: Option<NaiveDate>,
    /// Match queue entries using metadata filter syntax.
    #[arg(long)]
    filter: Option<Filter>,
    /// Print a headerless, tab-separated row for piping to other programs.
    #[arg(long, conflicts_with = "wrap")]
    plain: bool,
    /// Wrap long table cells instead of truncating them.
    #[arg(long, conflicts_with = "plain")]
    wrap: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, conflicts_with = "cards_only")]
    notes_only: bool,
    #[arg(long, conflicts_with = "notes_only")]
    cards_only: bool,
    #[arg(long)]
    limit: Option<usize>,
    /// Match entries using scalar, set, and boolean expressions.
    #[arg(long)]
    filter: Option<Filter>,
    #[arg(long, value_parser = parse_date)]
    as_of: Option<NaiveDate>,
    /// Print headerless, tab-separated rows for piping to other programs.
    #[arg(long, conflicts_with = "paths")]
    plain: bool,
    /// Print only root-relative file paths, one per line.
    #[arg(long, conflicts_with = "plain")]
    paths: bool,
    /// Wrap long table cells instead of truncating them.
    #[arg(long, conflicts_with_all = ["plain", "paths"])]
    wrap: bool,
}

/// Execute parsed command-line arguments.
pub fn run(cli: Cli) -> Result<(), String> {
    run_with_clock(cli, &SystemClock)
}

/// Execute with an injected clock for deterministic tests.
pub fn run_with_clock(cli: Cli, clock: &dyn Clock) -> Result<(), String> {
    match cli.command {
        Command::Audit { command } => run_audit(command),
        Command::FormatList { field, style, root } => {
            let style = match style {
                ListStyle::Flow => crate::frontmatter_list::Style::Flow,
                ListStyle::Block => crate::frontmatter_list::Style::Block,
                ListStyle::Toggle => crate::frontmatter_list::Style::Toggle,
            };
            let paths = read_update_paths(Path::new("-"))?;
            let count = crate::frontmatter_list::format_paths(&root, &paths, &field, style)?;
            println!(
                "updated {count} {}",
                if count == 1 { "file" } else { "files" }
            );
            Ok(())
        }
        Command::Import { command } => run_import(command),
        Command::Position {
            file,
            end_line,
            date,
        } => {
            let date = date.unwrap_or_else(|| clock.today());
            let edit = crate::document::append_note_event(&file, end_line, date)?;
            let incremented = if edit.pass_incremented {
                " pass_incremented"
            } else {
                ""
            };
            println!(
                "updated {}: date={date} end_line={} pass={}{}",
                file.display(),
                edit.stored_end_line,
                edit.pass,
                incremented
            );
            Ok(())
        }
        Command::Rate { file, rating, date } => {
            let date = date.unwrap_or_else(|| clock.today());
            crate::document::append_card_event(&file, rating, date)?;
            let document = crate::document::read(&file)?;
            let events = match document.history {
                Some(History::Card(events)) => events,
                _ => return Err("updated card could not be replayed".to_owned()),
            };
            let schedule = schedule_card(&events, date, CardSchedulerConfig::default())?;
            println!(
                "rated {}: rating={rating} next={} interval={}d",
                file.display(),
                schedule.metrics.due_date,
                schedule.metrics.interval_days.unwrap()
            );
            Ok(())
        }
        Command::Queue => run_list(
            Path::new("."),
            clock.today(),
            QueueOptions::default(),
            false,
            false,
            false,
            None,
        ),
        Command::Next(arguments) => run_list(
            &arguments.root,
            arguments.as_of.unwrap_or_else(|| clock.today()),
            QueueOptions {
                limit: Some(1),
                ..QueueOptions::default()
            },
            arguments.plain,
            false,
            arguments.wrap,
            arguments.filter.as_ref(),
        ),
        Command::List(arguments) => run_list(
            &arguments.root,
            arguments.as_of.unwrap_or_else(|| clock.today()),
            QueueOptions {
                notes_only: arguments.notes_only,
                cards_only: arguments.cards_only,
                include_upcoming: true,
                limit: arguments.limit,
            },
            arguments.plain,
            arguments.paths,
            arguments.wrap,
            arguments.filter.as_ref(),
        ),
        Command::Update { command } => run_update(command),
    }
}

fn run_update(command: UpdateCommand) -> Result<(), String> {
    let count = match command {
        UpdateCommand::Priority {
            priority,
            selection,
        } => {
            let paths = read_update_paths(&selection.files_from)?;
            crate::update::priority(&selection.root, &paths, priority)?
        }
        UpdateCommand::Tags { command } => match command {
            TagUpdateCommand::Add {
                tags,
                existing,
                selection,
            } => {
                let paths = read_update_paths(&selection.files_from)?;
                crate::update::tags_add(
                    &selection.root,
                    &paths,
                    &tags,
                    matches!(existing, ExistingTags::Overwrite),
                )?
            }
            TagUpdateCommand::Rename {
                from,
                to,
                selection,
            } => {
                let paths = read_update_paths(&selection.files_from)?;
                crate::update::tags_rename(&selection.root, &paths, &from, &to)?
            }
            TagUpdateCommand::Remove { tags, selection } => {
                let paths = read_update_paths(&selection.files_from)?;
                crate::update::tags_remove(&selection.root, &paths, &tags)?
            }
        },
    };
    println!(
        "updated {count} {}",
        if count == 1 { "file" } else { "files" }
    );
    Ok(())
}

fn read_update_paths(files_from: &Path) -> Result<Vec<PathBuf>, String> {
    let mut contents = String::new();
    if files_from == Path::new("-") {
        io::stdin()
            .read_to_string(&mut contents)
            .map_err(|error| format!("cannot read file paths from standard input: {error}"))?;
    } else {
        contents = fs::read_to_string(files_from)
            .map_err(|error| format!("{}: {error}", files_from.display()))?;
    }
    Ok(contents
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn run_import(command: ImportCommand) -> Result<(), String> {
    let ImportCommand::Anki { file, output } = command;
    let output = match output {
        Some(output) => output,
        None => crate::import::anki::default_output_path(&file)?,
    };
    let report = crate::import::anki::import(&file, &output)?;
    for event in &report.events {
        println!("{event}");
    }
    println!(
        "imported {} cards and {} media files into {}; skipped {} cards and {} media files",
        report.imported_cards,
        report.imported_media,
        output.display(),
        report.skipped_cards,
        report.skipped_media,
    );
    if report.errors.is_empty() {
        Ok(())
    } else {
        for error in &report.errors {
            eprintln!("error: {error}");
        }
        Err(format!(
            "import incomplete: {} error(s); fix the errors and rerun the same command to resume",
            report.errors.len()
        ))
    }
}

fn run_audit(command: AuditCommand) -> Result<(), String> {
    let (root, missing) = match command {
        AuditCommand::Missing(arguments) => (arguments.root, true),
        AuditCommand::Invalid(arguments) => (arguments.root, false),
    };
    for path in markdown_files(&root)? {
        let relative_path = relative(&root, &path);
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) => {
                if !missing {
                    println!(
                        "{} [utf8-invalid] file is not valid UTF-8",
                        relative_path.display()
                    );
                }
                continue;
            }
        };
        let document = parse(&relative_path, source);
        if missing && document.classification() == Classification::Missing {
            println!(
                "{} missing: {}",
                relative_path.display(),
                document.missing.join(", ")
            );
        } else if !missing && document.classification() == Classification::Invalid {
            for diagnostic in document.diagnostics {
                println!("{diagnostic}");
            }
        }
    }
    Ok(())
}

fn run_list(
    root: &Path,
    as_of: NaiveDate,
    options: QueueOptions,
    plain: bool,
    paths: bool,
    wrap: bool,
    filter: Option<&Filter>,
) -> Result<(), String> {
    let result = build(root, as_of, options, filter)?;
    let output = if paths {
        crate::output::paths(&result.items)
    } else if plain {
        crate::output::queue_plain(&result.items)
    } else {
        crate::output::queue(&result.items, wrap)
    };
    print!("{output}");
    if result.diagnostics.is_empty() {
        return Ok(());
    }
    for diagnostic in &result.diagnostics {
        eprintln!("{diagnostic}");
    }
    let count = result
        .diagnostics
        .iter()
        .map(|diagnostic| &diagnostic.path)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    Err(format!(
        "{count} invalid files skipped; run 'retent audit invalid'"
    ))
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    if value.len() != 10 {
        return Err("expected date YYYY-MM-DD".to_owned());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("invalid calendar date {value:?}; expected YYYY-MM-DD"))
}

//! Command-line structure and orchestration.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};

use crate::clock::{Clock, SystemClock};
use crate::discover::{markdown_files, relative};
use crate::document::{Classification, History, parse};
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
    Queue(QueueArgs),
    /// Print the first unified queue item.
    Next(NextArgs),
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
struct QueueArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, conflicts_with = "cards_only")]
    notes_only: bool,
    #[arg(long, conflicts_with = "notes_only")]
    cards_only: bool,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long, value_parser = parse_date)]
    as_of: Option<NaiveDate>,
    /// Print headerless, tab-separated rows for piping to other programs.
    #[arg(long)]
    plain: bool,
}

#[derive(Debug, Args)]
struct NextArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, value_parser = parse_date)]
    as_of: Option<NaiveDate>,
    /// Print a headerless, tab-separated row for piping to other programs.
    #[arg(long)]
    plain: bool,
}

/// Execute parsed command-line arguments.
pub fn run(cli: Cli) -> Result<(), String> {
    run_with_clock(cli, &SystemClock)
}

/// Execute with an injected clock for deterministic tests.
pub fn run_with_clock(cli: Cli, clock: &dyn Clock) -> Result<(), String> {
    match cli.command {
        Command::Audit { command } => run_audit(command),
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
        Command::Queue(arguments) => run_queue(
            &arguments.root,
            arguments.as_of.unwrap_or_else(|| clock.today()),
            QueueOptions {
                notes_only: arguments.notes_only,
                cards_only: arguments.cards_only,
                include_upcoming: arguments.all,
                limit: arguments.limit,
            },
            arguments.plain,
        ),
        Command::Next(arguments) => run_queue(
            &arguments.root,
            arguments.as_of.unwrap_or_else(|| clock.today()),
            QueueOptions {
                limit: Some(1),
                ..QueueOptions::default()
            },
            arguments.plain,
        ),
    }
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

fn run_queue(
    root: &Path,
    as_of: NaiveDate,
    options: QueueOptions,
    plain: bool,
) -> Result<(), String> {
    let result = build(root, as_of, options)?;
    let output = if plain {
        crate::output::queue_plain(&result.items)
    } else {
        crate::output::queue(&result.items)
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

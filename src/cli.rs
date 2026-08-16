//! Command-line structure, configuration layering, and orchestration.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::clock::{Clock, SystemClock};
use crate::config::{
    ItemType, LoadOptions, OutputFormat, RuntimeConfig, SchedulerOverrides, ViewOverrides,
    ViewSettings,
};
use crate::discover::{markdown_files, relative};
use crate::document::{Classification, History, parse};
use crate::scheduling::queue::{QueueOptions, build};

/// Markdown-native incremental learning.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Vault containing Markdown documents.
    #[arg(long, global = true, value_name = "PATH")]
    vault: Option<PathBuf>,
    /// Load this TOML file instead of discovering .retent.toml.
    #[arg(long, global = true, value_name = "FILE", conflicts_with = "no_config")]
    config: Option<PathBuf>,
    /// Disable configuration discovery and use built-in defaults.
    #[arg(long, global = true, conflicts_with = "config")]
    no_config: bool,
    #[command(flatten)]
    scheduling: SchedulingArgs,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct SchedulingArgs {
    /// Override FSRS desired retention.
    #[arg(
        long,
        global = true,
        value_name = "RATIO",
        help_heading = "Scheduling overrides"
    )]
    card_retention: Option<f32>,
    /// Override the maximum note interval in days.
    #[arg(
        long,
        global = true,
        value_name = "DAYS",
        help_heading = "Scheduling overrides"
    )]
    note_max_interval: Option<u32>,
    /// Override the note exposure half-life in days.
    #[arg(
        long,
        global = true,
        value_name = "DAYS",
        help_heading = "Scheduling overrides"
    )]
    note_exposure_half_life: Option<f64>,
    /// Override the note pass interval multiplier.
    #[arg(
        long,
        global = true,
        value_name = "FACTOR",
        help_heading = "Scheduling overrides"
    )]
    note_pass_multiplier: Option<f64>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Audit Markdown classification.
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Show effective configuration and the source of every value.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Format a frontmatter sequence in selected paths.
    FormatList {
        /// Frontmatter field containing the sequence.
        field: String,
        /// Sequence syntax to produce.
        #[arg(long, value_enum)]
        style: ListStyle,
        #[command(flatten)]
        selection: FileSelection,
    },
    /// Import cards from another application.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Record progress through a note.
    #[command(alias = "position")]
    Progress {
        file: PathBuf,
        /// Last line presented during this reading event.
        #[arg(long)]
        end_line: u32,
        /// Backdate the event instead of using today.
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// Record a card rating from 1 (Again) to 4 (Easy).
    Rate {
        file: PathBuf,
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        rating: u8,
        /// Backdate the event instead of using today.
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    /// Print the due learning queue.
    Queue(QueryArgs),
    /// Print the first due queue item.
    Next(ViewArgs),
    /// List all scheduled entries, including upcoming items.
    List(QueryArgs),
    /// Update metadata on paths read from a file or standard input.
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print effective values and whether each came from built-ins, TOML, or CLI.
    Show,
}

#[derive(Debug, Subcommand)]
enum UpdateCommand {
    /// Set priority on every selected entry.
    Priority {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=10))]
        priority: u8,
        #[command(flatten)]
        selection: FileSelection,
    },
    /// Update tags on every selected entry.
    Tags {
        #[command(subcommand)]
        command: TagUpdateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TagUpdateCommand {
    /// Add tags while retaining existing tags.
    Add {
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[command(flatten)]
        selection: FileSelection,
    },
    /// Replace the complete tag list.
    Set {
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[command(flatten)]
        selection: FileSelection,
    },
    /// Rename a tag, removing duplicates if the destination already exists.
    Rename {
        from: String,
        to: String,
        #[command(flatten)]
        selection: FileSelection,
    },
    /// Remove one or more tags.
    Remove {
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[command(flatten)]
        selection: FileSelection,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ListStyle {
    Flow,
    Block,
    Toggle,
}

#[derive(Debug, Args)]
struct FileSelection {
    /// Read newline-delimited paths from this file, or `-` for standard input.
    #[arg(long, value_name = "FILE")]
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
    Missing,
    /// Show documents with syntax or semantic errors.
    Invalid,
}

#[derive(Debug, Args)]
struct QueryArgs {
    #[command(flatten)]
    view: ViewArgs,
    /// Show at most this many entries.
    #[arg(long, conflicts_with = "no_limit")]
    limit: Option<usize>,
    /// Remove a limit supplied by configuration or built-in defaults.
    #[arg(long, conflicts_with = "limit")]
    no_limit: bool,
}

#[derive(Debug, Args)]
struct ViewArgs {
    /// Evaluate scheduling as of this date instead of today.
    #[arg(long, value_parser = parse_date)]
    as_of: Option<NaiveDate>,
    /// Match entries using scalar, set, and boolean expressions.
    #[arg(long, conflicts_with = "no_filter")]
    filter: Option<String>,
    /// Remove a filter supplied by configuration.
    #[arg(long, conflicts_with = "filter")]
    no_filter: bool,
    /// Select all entries, notes, or cards.
    #[arg(long, value_enum, value_name = "KIND")]
    r#type: Option<ItemType>,
    /// Select table, TSV, path-only, or JSON output.
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,
    /// Wrap long table cells.
    #[arg(long, conflicts_with = "no_wrap")]
    wrap: bool,
    /// Disable wrapping supplied by configuration.
    #[arg(long, conflicts_with = "wrap")]
    no_wrap: bool,
    /// Emit valid entries despite invalid files in the vault.
    #[arg(long)]
    allow_invalid: bool,
}

/// Execute parsed command-line arguments.
pub fn run(cli: Cli) -> Result<(), String> {
    run_with_clock(cli, &SystemClock)
}

/// Execute with an injected clock for deterministic tests.
pub fn run_with_clock(cli: Cli, clock: &dyn Clock) -> Result<(), String> {
    let runtime = crate::config::resolve(
        LoadOptions {
            vault: cli.vault,
            config: cli.config,
            no_config: cli.no_config,
        },
        SchedulerOverrides {
            card_retention: cli.scheduling.card_retention,
            note_max_interval: cli.scheduling.note_max_interval,
            note_exposure_half_life: cli.scheduling.note_exposure_half_life,
            note_pass_multiplier: cli.scheduling.note_pass_multiplier,
        },
    )?;

    match cli.command {
        Command::Audit { command } => run_audit(command, &runtime.vault.value),
        Command::Config { command } => match command {
            ConfigCommand::Show => {
                print!("{}", runtime.show());
                Ok(())
            }
        },
        Command::FormatList {
            field,
            style,
            selection,
        } => {
            let style = match style {
                ListStyle::Flow => crate::frontmatter_list::Style::Flow,
                ListStyle::Block => crate::frontmatter_list::Style::Block,
                ListStyle::Toggle => crate::frontmatter_list::Style::Toggle,
            };
            let paths = read_selected_paths(&selection.files_from)?;
            let count =
                crate::frontmatter_list::format_paths(&runtime.vault.value, &paths, &field, style)?;
            print_updated(count);
            Ok(())
        }
        Command::Import { command } => run_import(command),
        Command::Progress {
            file,
            end_line,
            date,
        } => {
            let date = date.unwrap_or_else(|| clock.today());
            let file = resolve_document_path(&runtime.vault.value, file);
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
            let file = resolve_document_path(&runtime.vault.value, file);
            crate::document::append_card_event(&file, rating, date)?;
            let document = crate::document::read(&file)?;
            let events = match document.history {
                Some(History::Card(events)) => events,
                _ => return Err("updated card could not be replayed".to_owned()),
            };
            let schedule = crate::scheduling::card::schedule(
                &events,
                date,
                runtime.scheduling.scheduler().card,
            )?;
            println!(
                "rated {}: rating={rating} next={} interval={}d",
                file.display(),
                schedule.metrics.due_date,
                schedule.metrics.interval_days.unwrap()
            );
            Ok(())
        }
        Command::Queue(arguments) => {
            let view = runtime.queue.with_overrides(query_overrides(&arguments))?;
            run_list(
                &runtime,
                arguments.view.as_of.unwrap_or_else(|| clock.today()),
                false,
                view,
                arguments.view.allow_invalid,
            )
        }
        Command::Next(arguments) => {
            let mut overrides = view_overrides(&arguments);
            overrides.limit = Some(Some(1));
            let view = runtime.queue.with_overrides(overrides)?;
            run_list(
                &runtime,
                arguments.as_of.unwrap_or_else(|| clock.today()),
                false,
                view,
                arguments.allow_invalid,
            )
        }
        Command::List(arguments) => {
            let view = runtime.list.with_overrides(query_overrides(&arguments))?;
            run_list(
                &runtime,
                arguments.view.as_of.unwrap_or_else(|| clock.today()),
                true,
                view,
                arguments.view.allow_invalid,
            )
        }
        Command::Update { command } => run_update(command, &runtime.vault.value),
    }
}

fn query_overrides(arguments: &QueryArgs) -> ViewOverrides {
    let mut overrides = view_overrides(&arguments.view);
    overrides.limit = if arguments.no_limit {
        Some(None)
    } else {
        arguments.limit.map(Some)
    };
    overrides
}

fn view_overrides(arguments: &ViewArgs) -> ViewOverrides {
    ViewOverrides {
        limit: None,
        filter: if arguments.no_filter {
            Some(None)
        } else {
            arguments.filter.clone().map(Some)
        },
        item_type: arguments.r#type,
        format: arguments.format,
        wrap: if arguments.no_wrap {
            Some(false)
        } else if arguments.wrap {
            Some(true)
        } else {
            None
        },
    }
}

fn run_update(command: UpdateCommand, vault: &Path) -> Result<(), String> {
    let count = match command {
        UpdateCommand::Priority {
            priority,
            selection,
        } => {
            let paths = read_selected_paths(&selection.files_from)?;
            crate::update::priority(vault, &paths, priority)?
        }
        UpdateCommand::Tags { command } => match command {
            TagUpdateCommand::Add { tags, selection } => {
                let paths = read_selected_paths(&selection.files_from)?;
                crate::update::tags_add(vault, &paths, &tags)?
            }
            TagUpdateCommand::Set { tags, selection } => {
                let paths = read_selected_paths(&selection.files_from)?;
                crate::update::tags_set(vault, &paths, &tags)?
            }
            TagUpdateCommand::Rename {
                from,
                to,
                selection,
            } => {
                let paths = read_selected_paths(&selection.files_from)?;
                crate::update::tags_rename(vault, &paths, &from, &to)?
            }
            TagUpdateCommand::Remove { tags, selection } => {
                let paths = read_selected_paths(&selection.files_from)?;
                crate::update::tags_remove(vault, &paths, &tags)?
            }
        },
    };
    print_updated(count);
    Ok(())
}

fn print_updated(count: usize) {
    println!(
        "updated {count} {}",
        if count == 1 { "file" } else { "files" }
    );
}

fn read_selected_paths(files_from: &Path) -> Result<Vec<PathBuf>, String> {
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
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect())
}

fn resolve_document_path(vault: &Path, file: PathBuf) -> PathBuf {
    if file.is_absolute() {
        file
    } else {
        vault.join(file)
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

fn run_audit(command: AuditCommand, vault: &Path) -> Result<(), String> {
    let missing = matches!(command, AuditCommand::Missing);
    for path in markdown_files(vault)? {
        let relative_path = relative(vault, &path);
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
    runtime: &RuntimeConfig,
    as_of: NaiveDate,
    include_upcoming: bool,
    view: ViewSettings,
    allow_invalid: bool,
) -> Result<(), String> {
    let filter = view.parsed_filter()?;
    let result = build(
        &runtime.vault.value,
        as_of,
        QueueOptions {
            element_type: view.item_type.value.element_type(),
            include_upcoming,
            limit: view.limit.value,
        },
        filter.as_ref(),
        runtime.scheduling.scheduler(),
    )?;

    if !result.diagnostics.is_empty() {
        for diagnostic in &result.diagnostics {
            eprintln!("{diagnostic}");
        }
        let message = invalid_summary(&result.diagnostics);
        if !allow_invalid {
            return Err(message);
        }
        eprintln!("warning: {message}; continuing because --allow-invalid was supplied");
    }

    let output = match view.format.value {
        OutputFormat::Table => crate::output::queue(&result.items, view.wrap.value),
        OutputFormat::Tsv => crate::output::queue_tsv(&result.items),
        OutputFormat::Paths => crate::output::paths(&result.items),
        OutputFormat::Json => crate::output::queue_json(&result.items),
    };
    print!("{output}");
    Ok(())
}

fn invalid_summary(diagnostics: &[crate::diagnostics::Diagnostic]) -> String {
    let count = diagnostics
        .iter()
        .map(|diagnostic| &diagnostic.path)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    format!("{count} invalid files skipped; run 'retent audit invalid'")
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    if value.len() != 10 {
        return Err("expected date YYYY-MM-DD".to_owned());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("invalid calendar date {value:?}; expected YYYY-MM-DD"))
}

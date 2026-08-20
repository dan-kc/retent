use std::fs;
use std::path::Path;

use clap::ValueEnum;
use fsrs::{DEFAULT_PARAMETERS, FSRS, FSRSItem, FSRSReview, current_retrievability};
use jiff::civil::Date;
use serde_yaml_ng::{Mapping, Value};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Column {
    #[value(name = "type")]
    Type,
    #[value(name = "priority")]
    Priority,
    #[value(name = "desired retention")]
    DesiredRetention,
    #[value(name = "predicted retention")]
    PredictedRetention,
    #[value(name = "difficulty")]
    Difficulty,
    #[value(name = "score")]
    Score,
}

pub(crate) enum Document {
    Managed(ManagedDocument),
    Unmanaged,
    Invalid(Vec<ValidationIssue>),
}

pub(crate) enum ManagedDocument {
    Note {
        priority: u64,
        score: f64,
    },
    Card {
        desired_retention: u64,
        memory: CardMemory,
        score: f32,
    },
}

pub(crate) struct ValidationIssue(String);

pub(crate) enum CardMemory {
    None,
    Valid {
        predicted_retention: f32,
        difficulty: f32,
        stability: f32,
        elapsed_days: u32,
    },
}

struct CardReview {
    date: Date,
    rating: u32,
}

struct NoteExposure {
    date: Date,
    pass: u64,
}

impl Document {
    pub(crate) fn read(path: &Path, today: Date) -> Self {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Self::Invalid(vec![ValidationIssue(format!(
                    "cannot read file: {error}"
                ))]);
            }
        };
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) => {
                return Self::Invalid(vec![ValidationIssue(
                    "file is not valid UTF-8".to_owned(),
                )]);
            }
        };
        Self::parse(&source, today)
    }

    fn parse(source: &str, today: Date) -> Self {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let mut lines = source.lines();
        if lines.next() != Some("---") {
            return Self::Unmanaged;
        }

        let mut yaml = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line == "---" {
                closed = true;
                break;
            }
            yaml.push_str(line);
            yaml.push('\n');
        }
        if !closed {
            return Self::Unmanaged;
        }

        let mapping = match serde_yaml_ng::from_str(&yaml) {
            Ok(Value::Mapping(mapping)) => mapping,
            Ok(_) | Err(_) => return Self::Unmanaged,
        };
        let body = lines.collect::<Vec<_>>().join("\n");

        match mapping.get(Value::String("type".to_owned())) {
            Some(Value::String(value)) if value == "note" => {
                validate_note(&mapping, &body, today)
            }
            Some(Value::String(value)) if value == "card" => {
                validate_card(&mapping, &body, today)
            }
            _ => Self::Unmanaged,
        }
    }

    pub(crate) fn values(&self, columns: &[Column]) -> Option<Vec<String>> {
        match self {
            Self::Managed(document) => Some(document.values(columns)),
            Self::Unmanaged => Some(columns.iter().map(|_| "-".to_owned()).collect()),
            Self::Invalid(issues) => {
                debug_assert!(issues.iter().all(|issue| !issue.0.is_empty()));
                None
            }
        }
    }
}

impl ManagedDocument {
    fn values(&self, columns: &[Column]) -> Vec<String> {
        columns
            .iter()
            .map(|column| self.value(*column))
            .collect()
    }

    fn value(&self, column: Column) -> String {
        match (self, column) {
            (Self::Note { .. }, Column::Type) => "note".to_owned(),
            (Self::Card { .. }, Column::Type) => "card".to_owned(),
            (Self::Note { priority, .. }, Column::Priority) => priority.to_string(),
            (
                Self::Card {
                    desired_retention, ..
                },
                Column::DesiredRetention,
            ) => desired_retention.to_string(),
            (Self::Card { memory, .. }, Column::PredictedRetention) => {
                predicted_retention_value(memory)
            }
            (Self::Card { memory, .. }, Column::Difficulty) => difficulty_value(memory),
            (Self::Note { score, .. }, Column::Score) => format!("{score:.3}"),
            (Self::Card { score, .. }, Column::Score) => format!("{score:.3}"),
            _ => "-".to_owned(),
        }
    }
}

fn validate_note(mapping: &Mapping, body: &str, today: Date) -> Document {
    let mut issues = Vec::new();
    let priority = match required_integer(mapping, "priority", 10) {
        Ok(priority) => Some(priority),
        Err(issue) => {
            issues.push(issue);
            None
        }
    };
    let exposures = match note_exposures(body, today) {
        Ok(exposures) => Some(exposures),
        Err(()) => {
            issues.push(ValidationIssue("invalid note history".to_owned()));
            None
        }
    };

    if !issues.is_empty() {
        return Document::Invalid(issues);
    }

    match (priority, exposures) {
        (Some(priority), Some(exposures)) => match calculate_note_score(priority, &exposures, today)
        {
            Ok(score) => Document::Managed(ManagedDocument::Note { priority, score }),
            Err(()) => Document::Invalid(vec![ValidationIssue(
                "note score could not be calculated".to_owned(),
            )]),
        },
        _ => Document::Invalid(vec![ValidationIssue(
            "note validation did not complete".to_owned(),
        )]),
    }
}

fn validate_card(mapping: &Mapping, body: &str, today: Date) -> Document {
    let mut issues = Vec::new();
    let desired_retention = match required_integer(mapping, "desired retention", 99) {
        Ok(desired_retention) => Some(desired_retention),
        Err(issue) => {
            issues.push(issue);
            None
        }
    };
    if let Err(issue) = validate_front_block(body) {
        issues.push(issue);
    }
    let reviews = match card_reviews(body, today) {
        Ok(reviews) => Some(reviews),
        Err(()) => {
            issues.push(ValidationIssue("invalid card history".to_owned()));
            None
        }
    };

    if !issues.is_empty() {
        return Document::Invalid(issues);
    }

    match (desired_retention, reviews) {
        (Some(desired_retention), Some(reviews)) => {
            let memory = if reviews.is_empty() {
                CardMemory::None
            } else {
                match calculate_card_memory(&reviews, today) {
                    Ok(memory) => memory,
                    Err(()) => {
                        return Document::Invalid(vec![ValidationIssue(
                            "card history could not be replayed".to_owned(),
                        )]);
                    }
                }
            };
            match calculate_card_score(desired_retention, &memory) {
                Ok(score) => Document::Managed(ManagedDocument::Card {
                    desired_retention,
                    memory,
                    score,
                }),
                Err(()) => Document::Invalid(vec![ValidationIssue(
                    "card score could not be calculated".to_owned(),
                )]),
            }
        }
        _ => Document::Invalid(vec![ValidationIssue(
            "card validation did not complete".to_owned(),
        )]),
    }
}

fn validate_front_block(body: &str) -> Result<(), ValidationIssue> {
    let mut completed = false;
    let mut open = false;

    for line in body.lines() {
        match line.trim() {
            "<!-- FRONT:BEGIN -->" => {
                if open {
                    return Err(ValidationIssue("card front block is nested".to_owned()));
                }
                if completed {
                    return Err(ValidationIssue(
                        "card has multiple front blocks".to_owned(),
                    ));
                }
                open = true;
            }
            "<!-- FRONT:END -->" => {
                if !open {
                    return Err(ValidationIssue(
                        "card front block ends before it begins".to_owned(),
                    ));
                }
                open = false;
                completed = true;
            }
            _ => {}
        }
    }

    if open {
        Err(ValidationIssue("card front block is unclosed".to_owned()))
    } else if completed {
        Ok(())
    } else {
        Err(ValidationIssue("card front block is missing".to_owned()))
    }
}

fn calculate_note_score(
    priority: u64,
    exposures: &[NoteExposure],
    today: Date,
) -> Result<f64, ()> {
    let priority_factor = priority as f64 / 10.0;
    let base_half_life = (11 - priority) as f64;
    let mut remaining_exposure = 0.0;

    for exposure in exposures {
        let age = days_between(exposure.date, today)? as f64;
        let inverse_pass_factor = (-(exposure.pass as f64)).exp2();
        remaining_exposure += (-(age / base_half_life * inverse_pass_factor)).exp2();
    }

    let score = priority_factor / (1.0 + remaining_exposure);
    if score.is_finite() && (0.0..=1.0).contains(&score) {
        Ok(score)
    } else {
        Err(())
    }
}

fn predicted_retention_value(memory: &CardMemory) -> String {
    match memory {
        CardMemory::None => "-".to_owned(),
        CardMemory::Valid {
            predicted_retention,
            ..
        } => {
            let percentage = (predicted_retention * 100.0).round().min(99.0);
            format!("{percentage:.0}")
        }
    }
}

fn difficulty_value(memory: &CardMemory) -> String {
    match memory {
        CardMemory::None => "-".to_owned(),
        CardMemory::Valid { difficulty, .. } => format!("{difficulty:.3}"),
    }
}

fn calculate_card_score(desired_retention: u64, memory: &CardMemory) -> Result<f32, ()> {
    match (desired_retention, memory) {
        (0, _) => Ok(0.0),
        (_, CardMemory::None) => Ok(0.5),
        (
            desired_retention,
            CardMemory::Valid {
                stability,
                elapsed_days,
                ..
            },
        ) => {
            let target_interval = FSRS::default().next_interval(
                Some(*stability),
                desired_retention as f32 / 100.0,
                3,
            );
            let elapsed_days = *elapsed_days as f32;
            let score = elapsed_days / (elapsed_days + target_interval);
            if target_interval.is_finite()
                && target_interval > 0.0
                && score.is_finite()
                && (0.0..=1.0).contains(&score)
            {
                Ok(score)
            } else {
                Err(())
            }
        }
    }
}

fn card_reviews(body: &str, today: Date) -> Result<Vec<CardReview>, ()> {
    match history_block(body)? {
        Some(lines) => parse_card_review_table(&lines, today),
        None => Ok(Vec::new()),
    }
}

fn note_exposures(body: &str, today: Date) -> Result<Vec<NoteExposure>, ()> {
    match history_block(body)? {
        Some(lines) => parse_note_exposure_table(&lines, today),
        None => Ok(Vec::new()),
    }
}

fn history_block(body: &str) -> Result<Option<Vec<&str>>, ()> {
    let mut completed_block = None;
    let mut current_block = None;

    for line in body.lines() {
        match line.trim() {
            "<!-- HISTORY:BEGIN -->" => {
                if current_block.is_some() || completed_block.is_some() {
                    return Err(());
                }
                current_block = Some(Vec::new());
            }
            "<!-- HISTORY:END -->" => {
                let block = current_block.take().ok_or(())?;
                if completed_block.replace(block).is_some() {
                    return Err(());
                }
            }
            _ => {
                if let Some(block) = &mut current_block {
                    block.push(line);
                }
            }
        }
    }

    if current_block.is_some() {
        return Err(());
    }

    Ok(completed_block)
}

fn parse_card_review_table(lines: &[&str], today: Date) -> Result<Vec<CardReview>, ()> {
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .ok_or(())?;
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .ok_or(())?;
    let table = &lines[first..=last];
    if table.len() < 2 || table.iter().any(|line| line.trim().is_empty()) {
        return Err(());
    }

    let header = table_cells(table[0]).ok_or(())?;
    if header != ["Date", "Rating"] {
        return Err(());
    }
    let separator = table_cells(table[1]).ok_or(())?;
    if separator.len() != 2 || !separator.iter().all(is_table_separator) {
        return Err(());
    }

    let mut previous_date = None;
    table[2..]
        .iter()
        .map(|line| {
            let cells = table_cells(line).ok_or(())?;
            if cells.len() != 2 {
                return Err(());
            }
            let date = parse_date(cells[0])?;
            let rating = parse_rating(cells[1])?;
            if date > today || previous_date.is_some_and(|previous| date < previous) {
                return Err(());
            }
            previous_date = Some(date);
            Ok(CardReview { date, rating })
        })
        .collect()
}

fn parse_note_exposure_table(lines: &[&str], today: Date) -> Result<Vec<NoteExposure>, ()> {
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .ok_or(())?;
    let last = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .ok_or(())?;
    let table = &lines[first..=last];
    if table.len() < 2 || table.iter().any(|line| line.trim().is_empty()) {
        return Err(());
    }

    let header = table_cells(table[0]).ok_or(())?;
    if header != ["Date", "End Line", "Pass"] {
        return Err(());
    }
    let separator = table_cells(table[1]).ok_or(())?;
    if separator.len() != 3 || !separator.iter().all(is_table_separator) {
        return Err(());
    }

    let mut previous_date = None;
    let mut previous_pass = None;
    table[2..]
        .iter()
        .map(|line| {
            let cells = table_cells(line).ok_or(())?;
            if cells.len() != 3 {
                return Err(());
            }
            let date = parse_date(cells[0])?;
            parse_u64(cells[1])?;
            let pass = parse_u64(cells[2])?;
            if date > today
                || previous_date.is_some_and(|previous| date < previous)
                || previous_pass.is_some_and(|previous| pass < previous)
            {
                return Err(());
            }
            previous_date = Some(date);
            previous_pass = Some(pass);
            Ok(NoteExposure { date, pass })
        })
        .collect()
}

fn table_cells(line: &str) -> Option<Vec<&str>> {
    let line = line.trim();
    let contents = line.strip_prefix('|')?.strip_suffix('|')?;
    Some(contents.split('|').map(str::trim).collect())
}

fn is_table_separator(cell: &&str) -> bool {
    let cell = cell.strip_prefix(':').unwrap_or(cell);
    let cell = cell.strip_suffix(':').unwrap_or(cell);
    cell.len() >= 3 && cell.bytes().all(|byte| byte == b'-')
}

fn parse_date(value: &str) -> Result<Date, ()> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn parse_rating(value: &str) -> Result<u32, ()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    match value.parse() {
        Ok(rating @ 1..=4) => Ok(rating),
        _ => Err(()),
    }
}

fn parse_u64(value: &str) -> Result<u64, ()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn calculate_card_memory(reviews: &[CardReview], today: Date) -> Result<CardMemory, ()> {
    let mut previous_date = None;
    let mut fsrs_reviews = Vec::with_capacity(reviews.len());
    for review in reviews {
        let delta_t = match previous_date {
            Some(previous) => days_between(previous, review.date)?,
            None => 0,
        };
        fsrs_reviews.push(FSRSReview {
            rating: review.rating,
            delta_t,
        });
        previous_date = Some(review.date);
    }

    let state = FSRS::default()
        .memory_state(
            FSRSItem {
                reviews: fsrs_reviews,
            },
            None,
        )
        .map_err(|_| ())?;
    let last_date = previous_date.ok_or(())?;
    let elapsed_days = days_between(last_date, today)?;
    let predicted_retention =
        current_retrievability(state, elapsed_days as f32, DEFAULT_PARAMETERS[20]);
    let difficulty = (state.difficulty - 1.0) / 9.0;
    if !predicted_retention.is_finite()
        || !(0.0..=1.0).contains(&predicted_retention)
        || !difficulty.is_finite()
        || !(0.0..=1.0).contains(&difficulty)
    {
        return Err(());
    }
    Ok(CardMemory::Valid {
        predicted_retention,
        difficulty,
        stability: state.stability,
        elapsed_days,
    })
}

fn days_between(start: Date, end: Date) -> Result<u32, ()> {
    u32::try_from((end - start).get_days()).map_err(|_| ())
}

fn required_integer(
    mapping: &Mapping,
    key: &str,
    maximum: u64,
) -> Result<u64, ValidationIssue> {
    let Some(value) = mapping.get(Value::String(key.to_owned())) else {
        return Err(ValidationIssue(format!("{key} is missing")));
    };
    match value.as_u64() {
        Some(value) if value <= maximum => Ok(value),
        _ => Err(ValidationIssue(format!(
            "{key} must be an unquoted integer from 0 to {maximum}"
        ))),
    }
}

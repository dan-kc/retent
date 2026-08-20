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
}

pub(crate) enum Frontmatter {
    Note {
        priority: RequiredInteger,
    },
    Card {
        desired_retention: RequiredInteger,
        body: String,
    },
    Other,
    Invalid,
}

pub(crate) enum RequiredInteger {
    Valid(u64),
    Invalid,
}

enum CardMemory {
    None,
    Valid {
        predicted_retention: f32,
        difficulty: f32,
    },
    Invalid,
}

struct CardReview {
    date: Date,
    rating: u32,
}

impl Column {
    pub(crate) fn needs_card_memory(self) -> bool {
        matches!(self, Self::PredictedRetention | Self::Difficulty)
    }
}

impl Frontmatter {
    pub(crate) fn read(path: &Path) -> Self {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(_) => return Self::Invalid,
        };
        Self::parse(&source)
    }

    fn parse(source: &str) -> Self {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let mut lines = source.lines();
        if lines.next() != Some("---") {
            return Self::Other;
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
            return Self::Invalid;
        }

        let mapping = match serde_yaml_ng::from_str(&yaml) {
            Ok(Value::Mapping(mapping)) => mapping,
            Ok(_) => return Self::Other,
            Err(_) => return Self::Invalid,
        };
        let body = lines.collect::<Vec<_>>().join("\n");

        match mapping.get(Value::String("type".to_owned())) {
            Some(Value::String(value)) if value == "note" => Self::Note {
                priority: required_integer(&mapping, "priority", 10),
            },
            Some(Value::String(value)) if value == "card" => Self::Card {
                desired_retention: required_integer(&mapping, "desired retention", 100),
                body,
            },
            _ => Self::Other,
        }
    }

    pub(crate) fn values(&self, columns: &[Column], today: Option<Date>) -> Vec<String> {
        let memory = match self {
            Self::Card { body, .. } if columns.iter().any(|column| column.needs_card_memory()) => {
                Some(CardMemory::from_body(body, today))
            }
            _ => None,
        };

        columns
            .iter()
            .map(|column| self.value(*column, memory.as_ref()))
            .collect()
    }

    fn value(&self, column: Column, memory: Option<&CardMemory>) -> String {
        match (self, column) {
            (Self::Note { .. }, Column::Type) => "note".to_owned(),
            (Self::Card { .. }, Column::Type) => "card".to_owned(),
            (Self::Note { priority }, Column::Priority) => priority.value(),
            (
                Self::Card {
                    desired_retention, ..
                },
                Column::DesiredRetention,
            ) => desired_retention.value(),
            (Self::Card { .. }, Column::PredictedRetention) => {
                memory_value(memory, |predicted_retention, _| predicted_retention)
            }
            (Self::Card { .. }, Column::Difficulty) => {
                memory_value(memory, |_, difficulty| difficulty)
            }
            (Self::Invalid, _) => "?".to_owned(),
            _ => "-".to_owned(),
        }
    }
}

impl RequiredInteger {
    fn value(&self) -> String {
        match self {
            Self::Valid(value) => value.to_string(),
            Self::Invalid => "?".to_owned(),
        }
    }
}

impl CardMemory {
    fn from_body(body: &str, today: Option<Date>) -> Self {
        let today = match today {
            Some(today) => today,
            None => return Self::Invalid,
        };
        let reviews = match card_reviews(body, today) {
            Ok(reviews) => reviews,
            Err(()) => return Self::Invalid,
        };
        if reviews.is_empty() {
            return Self::None;
        }

        match calculate_card_memory(&reviews, today) {
            Ok((predicted_retention, difficulty)) => Self::Valid {
                predicted_retention,
                difficulty,
            },
            Err(()) => Self::Invalid,
        }
    }
}

fn memory_value(memory: Option<&CardMemory>, select: impl FnOnce(f32, f32) -> f32) -> String {
    match memory {
        Some(CardMemory::None) => "-".to_owned(),
        Some(CardMemory::Valid {
            predicted_retention,
            difficulty,
        }) => format!("{:.3}", select(*predicted_retention, *difficulty)),
        Some(CardMemory::Invalid) | None => "?".to_owned(),
    }
}

fn card_reviews(body: &str, today: Date) -> Result<Vec<CardReview>, ()> {
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

    match completed_block {
        Some(lines) => parse_card_review_table(&lines, today),
        None => Ok(Vec::new()),
    }
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

fn calculate_card_memory(reviews: &[CardReview], today: Date) -> Result<(f32, f32), ()> {
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
    let elapsed = days_between(last_date, today)? as f32;
    let predicted_retention = current_retrievability(state, elapsed, DEFAULT_PARAMETERS[20]);
    let difficulty = (state.difficulty - 1.0) / 9.0;
    if !predicted_retention.is_finite()
        || !(0.0..=1.0).contains(&predicted_retention)
        || !difficulty.is_finite()
        || !(0.0..=1.0).contains(&difficulty)
    {
        return Err(());
    }
    Ok((predicted_retention, difficulty))
}

fn days_between(start: Date, end: Date) -> Result<u32, ()> {
    u32::try_from((end - start).get_days()).map_err(|_| ())
}

fn required_integer(mapping: &Mapping, key: &str, maximum: u64) -> RequiredInteger {
    match mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_u64)
    {
        Some(value) if value <= maximum => RequiredInteger::Valid(value),
        _ => RequiredInteger::Invalid,
    }
}

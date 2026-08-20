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

pub(crate) enum Frontmatter {
    Note {
        priority: RequiredInteger,
        body: String,
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
        stability: f32,
        elapsed_days: u32,
    },
    Invalid,
}

enum NoteScore {
    Valid(f64),
    Invalid,
}

struct CardReview {
    date: Date,
    rating: u32,
}

struct NoteExposure {
    date: Date,
    pass: u64,
}

impl Column {
    fn needs_card_memory(self) -> bool {
        matches!(self, Self::PredictedRetention | Self::Difficulty)
    }

    pub(crate) fn needs_today(self) -> bool {
        self.needs_card_memory() || matches!(self, Self::Score)
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
                body,
            },
            Some(Value::String(value)) if value == "card" => Self::Card {
                desired_retention: required_integer(&mapping, "desired retention", 99),
                body,
            },
            _ => Self::Other,
        }
    }

    pub(crate) fn values(&self, columns: &[Column], today: Option<Date>) -> Vec<String> {
        let card_memory = match self {
            Self::Card {
                desired_retention,
                body,
            } if needs_card_memory(columns, desired_retention) => {
                Some(CardMemory::from_body(body, today))
            }
            _ => None,
        };
        let note_score = match self {
            Self::Note { priority, body }
                if columns.iter().any(|column| matches!(column, Column::Score)) =>
            {
                Some(NoteScore::from_body(priority, body, today))
            }
            _ => None,
        };

        columns
            .iter()
            .map(|column| self.value(*column, card_memory.as_ref(), note_score.as_ref()))
            .collect()
    }

    fn value(
        &self,
        column: Column,
        card_memory: Option<&CardMemory>,
        note_score: Option<&NoteScore>,
    ) -> String {
        match (self, column) {
            (Self::Note { .. }, Column::Type) => "note".to_owned(),
            (Self::Card { .. }, Column::Type) => "card".to_owned(),
            (Self::Note { priority, .. }, Column::Priority) => priority.value(),
            (
                Self::Card {
                    desired_retention, ..
                },
                Column::DesiredRetention,
            ) => desired_retention.value(),
            (Self::Card { .. }, Column::PredictedRetention) => {
                predicted_retention_value(card_memory)
            }
            (Self::Card { .. }, Column::Difficulty) => difficulty_value(card_memory),
            (
                Self::Card {
                    desired_retention, ..
                },
                Column::Score,
            ) => card_score(desired_retention, card_memory),
            (Self::Note { .. }, Column::Score) => note_score_value(note_score),
            (Self::Invalid, _) => "?".to_owned(),
            _ => "-".to_owned(),
        }
    }
}

fn needs_card_memory(columns: &[Column], desired_retention: &RequiredInteger) -> bool {
    columns.iter().any(|column| {
        column.needs_card_memory()
            || matches!(
                (column, desired_retention),
                (Column::Score, RequiredInteger::Valid(1..=99))
            )
    })
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
            Ok(memory) => memory,
            Err(()) => Self::Invalid,
        }
    }
}

impl NoteScore {
    fn from_body(priority: &RequiredInteger, body: &str, today: Option<Date>) -> Self {
        let (priority, today) = match (priority, today) {
            (RequiredInteger::Valid(priority), Some(today)) => (*priority, today),
            _ => return Self::Invalid,
        };
        let exposures = match note_exposures(body, today) {
            Ok(exposures) => exposures,
            Err(()) => return Self::Invalid,
        };
        let priority_factor = priority as f64 / 10.0;
        let base_half_life = (11 - priority) as f64;
        let mut remaining_exposure = 0.0;

        for exposure in exposures {
            let age = match days_between(exposure.date, today) {
                Ok(age) => age as f64,
                Err(()) => return Self::Invalid,
            };
            let inverse_pass_factor = (-(exposure.pass as f64)).exp2();
            remaining_exposure += (-(age / base_half_life * inverse_pass_factor)).exp2();
        }

        let score = priority_factor / (1.0 + remaining_exposure);
        if score.is_finite() && (0.0..=1.0).contains(&score) {
            Self::Valid(score)
        } else {
            Self::Invalid
        }
    }
}

fn predicted_retention_value(memory: Option<&CardMemory>) -> String {
    match memory {
        Some(CardMemory::None) => "-".to_owned(),
        Some(CardMemory::Valid {
            predicted_retention,
            ..
        }) => {
            let percentage = (predicted_retention * 100.0).round().min(99.0);
            format!("{percentage:.0}")
        }
        Some(CardMemory::Invalid) | None => "?".to_owned(),
    }
}

fn difficulty_value(memory: Option<&CardMemory>) -> String {
    match memory {
        Some(CardMemory::None) => "-".to_owned(),
        Some(CardMemory::Valid { difficulty, .. }) => format!("{difficulty:.3}"),
        Some(CardMemory::Invalid) | None => "?".to_owned(),
    }
}

fn note_score_value(score: Option<&NoteScore>) -> String {
    match score {
        Some(NoteScore::Valid(score)) => format!("{score:.3}"),
        Some(NoteScore::Invalid) | None => "?".to_owned(),
    }
}

fn card_score(desired_retention: &RequiredInteger, memory: Option<&CardMemory>) -> String {
    match (desired_retention, memory) {
        (RequiredInteger::Invalid, _) => "?".to_owned(),
        (RequiredInteger::Valid(0), _) => "0.000".to_owned(),
        (RequiredInteger::Valid(_), Some(CardMemory::None)) => "0.500".to_owned(),
        (
            RequiredInteger::Valid(desired_retention),
            Some(CardMemory::Valid {
                stability,
                elapsed_days,
                ..
            }),
        ) => {
            let target_interval = FSRS::default().next_interval(
                Some(*stability),
                *desired_retention as f32 / 100.0,
                3,
            );
            let elapsed_days = *elapsed_days as f32;
            let score = elapsed_days / (elapsed_days + target_interval);
            if target_interval.is_finite()
                && target_interval > 0.0
                && score.is_finite()
                && (0.0..=1.0).contains(&score)
            {
                format!("{score:.3}")
            } else {
                "?".to_owned()
            }
        }
        (RequiredInteger::Valid(_), Some(CardMemory::Invalid) | None) => "?".to_owned(),
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

fn required_integer(mapping: &Mapping, key: &str, maximum: u64) -> RequiredInteger {
    match mapping
        .get(Value::String(key.to_owned()))
        .and_then(Value::as_u64)
    {
        Some(value) if value <= maximum => RequiredInteger::Valid(value),
        _ => RequiredInteger::Invalid,
    }
}

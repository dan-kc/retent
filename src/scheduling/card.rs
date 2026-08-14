//! FSRS card-state reconstruction from Markdown rating history.

use chrono::NaiveDate;
use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, ItemState, MemoryState, current_retrievability};

use crate::document::CardEvent;

use super::{ScheduleMetrics, new_metrics, reviewed_metrics};

/// FSRS scheduler settings.
#[derive(Debug, Clone, Copy)]
pub struct CardSchedulerConfig {
    pub desired_retention: f32,
}

impl Default for CardSchedulerConfig {
    fn default() -> Self {
        Self {
            desired_retention: 0.85,
        }
    }
}

/// Reconstructed card details for queue display.
#[derive(Debug, Clone)]
pub struct CardSchedule {
    pub metrics: ScheduleMetrics,
    pub retrievability: Option<f32>,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub last_rating: Option<u8>,
}

/// Replay all card reviews in table order using official FSRS defaults.
pub fn schedule(
    events: &[CardEvent],
    as_of: NaiveDate,
    config: CardSchedulerConfig,
) -> Result<CardSchedule, String> {
    if events.is_empty() {
        return Ok(CardSchedule {
            metrics: new_metrics(as_of),
            retrievability: None,
            stability: None,
            difficulty: None,
            last_rating: None,
        });
    }

    let fsrs = FSRS::default();
    let mut memory: Option<MemoryState> = None;
    let mut selected_interval = 1.0_f32;
    let mut previous_date = None;
    for event in events {
        let elapsed = previous_date
            .map(|previous: NaiveDate| (event.date - previous).num_days() as u32)
            .unwrap_or(0);
        let states = fsrs
            .next_states(memory, config.desired_retention, elapsed)
            .map_err(|error| format!("FSRS replay failed: {error}"))?;
        let selected = select_state(states, event.raw_rating);
        memory = Some(selected.memory);
        selected_interval = selected.interval;
        previous_date = Some(event.date);
    }

    let interval_days = selected_interval.round().max(1.0) as u32;
    let last_date = events.last().unwrap().date;
    let metrics = reviewed_metrics(last_date, interval_days, as_of);
    let state = memory.unwrap();
    let elapsed_as_of = (as_of - last_date).num_days().max(0) as f32;
    Ok(CardSchedule {
        metrics,
        retrievability: Some(current_retrievability(
            state,
            elapsed_as_of,
            FSRS6_DEFAULT_DECAY,
        )),
        stability: Some(state.stability),
        difficulty: Some(state.difficulty),
        last_rating: events.last().map(|event| event.raw_rating),
    })
}

fn select_state(states: fsrs::NextStates, raw_rating: u8) -> ItemState {
    match raw_rating {
        0 | 1 => states.again,
        2 => states.hard,
        3 => states.good,
        4 => states.easy,
        _ => states.again,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn new_card_is_due_immediately() {
        let result = schedule(&[], date("2026-08-14"), CardSchedulerConfig::default()).unwrap();
        assert_eq!(result.metrics.status, super::super::Status::New);
        assert_eq!(result.metrics.due_date, date("2026-08-14"));
    }

    #[test]
    fn same_day_replay_matches_frozen_golden() {
        let events = vec![
            CardEvent {
                date: date("2026-08-01"),
                raw_rating: 3,
                source_line: 1,
            },
            CardEvent {
                date: date("2026-08-01"),
                raw_rating: 4,
                source_line: 2,
            },
        ];
        let result = schedule(&events, date("2026-08-14"), CardSchedulerConfig::default()).unwrap();
        assert_eq!(result.metrics.interval_days, Some(8));
        assert_eq!(result.metrics.due_date, date("2026-08-09"));
        assert_eq!(result.stability, Some(3.946_054_2));
        assert_eq!(result.difficulty, Some(1.0));
        assert_eq!(CardSchedulerConfig::default().desired_retention, 0.85);
    }

    #[test]
    fn raw_ratings_select_the_expected_transition() {
        let states = FSRS::default().next_states(None, 0.85, 0).unwrap();
        assert_eq!(select_state(states.clone(), 0), states.again.clone());
        assert_eq!(select_state(states.clone(), 1), states.again.clone());
        assert_eq!(select_state(states.clone(), 2), states.hard.clone());
        assert_eq!(select_state(states.clone(), 3), states.good.clone());
        assert_eq!(select_state(states.clone(), 4), states.easy.clone());
        assert_eq!(select_state(states.clone(), u8::MAX), states.again);
    }
}

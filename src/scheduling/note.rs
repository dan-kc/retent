//! Topic cadence scheduler for incremental reading notes.

use chrono::NaiveDate;

use crate::document::NoteEvent;

use super::{ScheduleMetrics, new_metrics, reviewed_metrics};

/// Note scheduler settings.
#[derive(Debug, Clone, Copy)]
pub struct NoteSchedulerConfig {
    pub maximum_interval_days: u32,
    pub exposure_half_life_days: f64,
    pub pass_multiplier: f64,
}

impl Default for NoteSchedulerConfig {
    fn default() -> Self {
        Self {
            maximum_interval_days: 3650,
            exposure_half_life_days: 30.0,
            pass_multiplier: 4.0,
        }
    }
}

/// Reconstructed topic details for queue display.
#[derive(Debug, Clone)]
pub struct NoteSchedule {
    pub metrics: ScheduleMetrics,
    pub pass: Option<u32>,
    pub reads_in_pass: u32,
    pub recent_exposure: f64,
    pub resume_line: Option<u32>,
}

/// Schedule a note using only priority, dates, pass, and presentation count.
pub fn schedule(
    events: &[NoteEvent],
    priority: u8,
    as_of: NaiveDate,
    config: NoteSchedulerConfig,
) -> NoteSchedule {
    let Some(latest) = events.last() else {
        return NoteSchedule {
            metrics: new_metrics(as_of),
            pass: None,
            reads_in_pass: 0,
            recent_exposure: 0.0,
            resume_line: None,
        };
    };

    let reads_in_pass = events
        .iter()
        .rev()
        .take_while(|event| event.pass == latest.pass)
        .count() as u32;
    let recent_exposure = recent_exposure(events, config.exposure_half_life_days);
    let interval_days = interval_days(
        priority,
        latest.pass,
        reads_in_pass,
        recent_exposure,
        config,
    );
    NoteSchedule {
        metrics: reviewed_metrics(latest.date, interval_days, as_of),
        pass: Some(latest.pass),
        reads_in_pass,
        recent_exposure,
        resume_line: Some(latest.end_line),
    }
}

/// Calculate the finite interval in log space to prevent pass overflow.
pub fn interval_days(
    priority: u8,
    pass: u32,
    presentations: u32,
    recent_exposure: f64,
    config: NoteSchedulerConfig,
) -> u32 {
    let p = priority as f64 / 100.0;
    let a_factor = 1.10 + 0.15 * p;
    let frequency_growth = 1.0 + 0.5 * (1.0 + recent_exposure).ln();
    let log_interval = 3.0 * p * 2.0_f64.ln()
        + presentations.saturating_sub(1) as f64 * a_factor.ln()
        + pass.saturating_sub(1) as f64 * config.pass_multiplier.ln()
        + frequency_growth.ln();
    let rounded = log_interval
        .clamp(0.0, (config.maximum_interval_days as f64).ln())
        .exp()
        .ceil() as u32;
    rounded.clamp(1, config.maximum_interval_days)
}

/// Exponentially weighted exposure before the latest event.
pub fn recent_exposure(events: &[NoteEvent], half_life_days: f64) -> f64 {
    let Some(latest) = events.last() else {
        return 0.0;
    };
    if events.len() == 1 {
        return 0.0;
    }
    events[..events.len() - 1]
        .iter()
        .map(|event| {
            let days = (latest.date - event.date).num_days() as f64;
            2.0_f64.powf(-days / half_life_days)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use chrono::{Days, NaiveDate};
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn lower_numeric_priority_is_no_longer(low in 0u8..=100, high in 0u8..=100) {
            let (low, high) = if low <= high {(low, high)} else {(high, low)};
            let config = NoteSchedulerConfig::default();
            prop_assert!(interval_days(low, 1, 1, 0.0, config) <= interval_days(high, 1, 1, 0.0, config));
        }

        #[test]
        fn increasing_pass_never_shortens(pass in 1u32..u32::MAX) {
            let config = NoteSchedulerConfig::default();
            prop_assert!(interval_days(50, pass, 2, 1.0, config) <= interval_days(50, pass + 1, 2, 1.0, config));
        }

        #[test]
        fn increasing_presentations_never_shortens(count in 1u32..u32::MAX) {
            let config = NoteSchedulerConfig::default();
            prop_assert!(interval_days(50, 1, count, 1.0, config) <= interval_days(50, 1, count + 1, 1.0, config));
        }

        #[test]
        fn exposure_never_shortens(value in 0.0f64..10000.0) {
            let config = NoteSchedulerConfig::default();
            prop_assert!(interval_days(50, 1, 2, value, config) <= interval_days(50, 1, 2, value + 1.0, config));
        }

        #[test]
        fn pressure_advances(days in 1u64..1000) {
            let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
            let event = NoteEvent { date: start, end_line: 10, pass: 1, source_line: 1 };
            let early = schedule(std::slice::from_ref(&event), 50, start, NoteSchedulerConfig::default());
            let late = schedule(&[event], 50, start.checked_add_days(Days::new(days)).unwrap(), NoteSchedulerConfig::default());
            prop_assert!(late.metrics.pressure >= early.metrics.pressure);
        }
    }

    #[test]
    fn changing_end_line_changes_no_ranking_state() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let first = NoteEvent {
            date,
            end_line: 1,
            pass: 1,
            source_line: 1,
        };
        let second = NoteEvent {
            end_line: 999,
            ..first.clone()
        };
        let config = NoteSchedulerConfig::default();
        let left = schedule(&[first], 10, date, config);
        let right = schedule(&[second], 10, date, config);
        assert_eq!(left.metrics, right.metrics);
    }

    #[test]
    fn new_note_is_due_immediately_without_resume_state() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 14).unwrap();
        let result = schedule(&[], 50, as_of, NoteSchedulerConfig::default());

        assert_eq!(result.metrics.status, super::super::Status::New);
        assert_eq!(result.metrics.due_date, as_of);
        assert_eq!(result.pass, None);
        assert_eq!(result.resume_line, None);
        assert_eq!(result.reads_in_pass, 0);
        assert_eq!(result.recent_exposure, 0.0);
    }

    #[test]
    fn more_recent_review_has_more_exposure() {
        let latest = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let event = |date| NoteEvent {
            date,
            end_line: 0,
            pass: 1,
            source_line: 1,
        };
        let old = [event(latest - chrono::Duration::days(60)), event(latest)];
        let recent = [event(latest - chrono::Duration::days(1)), event(latest)];
        assert!(recent_exposure(&recent, 30.0) > recent_exposure(&old, 30.0));
    }

    #[test]
    fn intervals_are_always_finite() {
        assert_eq!(
            interval_days(
                100,
                u32::MAX,
                u32::MAX,
                f64::MAX,
                NoteSchedulerConfig::default()
            ),
            3650
        );
    }
}

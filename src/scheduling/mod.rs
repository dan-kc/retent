//! Pure type-specific schedulers and their unified queue.

pub mod card;
pub mod note;
pub mod queue;

use chrono::NaiveDate;

/// Convert the user-facing 1–10 priority to the scheduler's 0.1–1.0 scale.
pub(crate) fn normalized_priority(priority: u8) -> f64 {
    f64::from(priority) / 10.0
}

/// Queue lifecycle state as of a supplied date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    New,
    Upcoming,
    Due,
    Overdue,
}

impl std::fmt::Display for Status {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => formatter.write_str("new"),
            Self::Upcoming => formatter.write_str("upcoming"),
            Self::Due => formatter.write_str("due"),
            Self::Overdue => formatter.write_str("overdue"),
        }
    }
}

/// Common metrics used to interleave notes and cards.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleMetrics {
    pub status: Status,
    pub last_date: Option<NaiveDate>,
    pub interval_days: Option<u32>,
    pub due_date: NaiveDate,
    pub age_days: u32,
    pub pressure: f64,
}

pub(crate) fn reviewed_metrics(
    last_date: NaiveDate,
    interval_days: u32,
    as_of: NaiveDate,
) -> ScheduleMetrics {
    let due_date = last_date
        .checked_add_days(chrono::Days::new(interval_days as u64))
        .unwrap_or(NaiveDate::MAX);
    let age_days = (as_of - last_date).num_days().max(0) as u32;
    let status = if due_date < as_of {
        Status::Overdue
    } else if due_date == as_of {
        Status::Due
    } else {
        Status::Upcoming
    };
    ScheduleMetrics {
        status,
        last_date: Some(last_date),
        interval_days: Some(interval_days),
        due_date,
        age_days,
        pressure: age_days as f64 / interval_days as f64,
    }
}

pub(crate) fn new_metrics(as_of: NaiveDate) -> ScheduleMetrics {
    ScheduleMetrics {
        status: Status::New,
        last_date: None,
        interval_days: None,
        due_date: as_of,
        age_days: 0,
        pressure: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn reviewed_status_tracks_the_due_date() {
        let last = date("2026-08-10");
        let cases = [
            ("2026-08-12", Status::Upcoming, 2),
            ("2026-08-13", Status::Due, 3),
            ("2026-08-14", Status::Overdue, 4),
        ];

        for (as_of, expected_status, expected_age) in cases {
            let metrics = reviewed_metrics(last, 3, date(as_of));
            assert_eq!(metrics.status, expected_status);
            assert_eq!(metrics.age_days, expected_age);
            assert_eq!(metrics.due_date, date("2026-08-13"));
        }
    }

    #[test]
    fn future_reviews_do_not_produce_negative_age() {
        let metrics = reviewed_metrics(date("2026-08-20"), 3, date("2026-08-14"));
        assert_eq!(metrics.status, Status::Upcoming);
        assert_eq!(metrics.age_days, 0);
        assert_eq!(metrics.pressure, 0.0);
    }
}

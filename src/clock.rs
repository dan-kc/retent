//! Date injection for deterministic commands and tests.

use chrono::{Local, NaiveDate};

/// Supplies the local calendar date.
pub trait Clock {
    fn today(&self) -> NaiveDate;
}

/// The machine's local clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
}

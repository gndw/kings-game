//! The date that walks the calendar. Seeded in `main` from `Calendar::start`; the
//! day-by-day rollover lives in `game::advancing_date`.

use super::calendar::Calendar;
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Resource, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    /// Runtime counter; defaults to 0 when read out of RON.
    #[serde(default)]
    pub tick_count: u64,
}

impl Date {
    /// Ordinal day since `(year: 1, month: 1, day: 1)`. Two dates compare by ordinal under the same calendar.
    pub fn ordinal(&self, cal: &Calendar) -> i64 {
        let dpy = i64::from(cal.days_per_year());
        let dpm = i64::from(cal.days_per_month);
        i64::from(self.year - 1) * dpy
            + i64::from(self.month - 1) * dpm
            + i64::from(self.day - 1)
    }

    /// Walk `days` forward, rolling over at month and year boundaries.
    pub fn after_days(&self, days: u32, cal: &Calendar) -> Date {
        let mut d = *self;
        let mut remaining = days;
        while remaining > 0 {
            if d.day < cal.days_per_month {
                d.day += 1;
            } else if d.month < cal.months_per_year {
                d.day = 1;
                d.month += 1;
            } else {
                d.day = 1;
                d.month = 1;
                d.year += 1;
            }
            remaining -= 1;
        }
        d
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

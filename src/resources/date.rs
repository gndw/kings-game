//! The date that walks the calendar. An ECS resource, seeded in `main` from
//! [`crate::resources::calendar::Calendar::start`]; the day-by-day rollover
//! lives in [`crate::updates::advance_date`].

use super::calendar::Calendar;
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Resource, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    /// How many ticks have elapsed since the game opened. Runtime counter;
    /// defaults to 0 when read out of RON (a calendar mod never needs to
    /// write it).
    #[serde(default)]
    pub tick_count: u64,
}

impl Date {
    /// The ordinal day since `(year: 1, month: 1, day: 1)`. Two dates compare
    /// true-for-now by comparing their ordinals under the same calendar —
    /// used by the construction-completion check and by anything else that
    /// needs a total order across arbitrary year/month/day triples.
    pub fn ordinal(&self, cal: &Calendar) -> i64 {
        let dpy = i64::from(cal.days_per_year());
        let dpm = i64::from(cal.days_per_month);
        i64::from(self.year - 1) * dpy
            + i64::from(self.month - 1) * dpm
            + i64::from(self.day - 1)
    }

    /// Walk `days` forward under `cal`, rolling over at month and year
    /// boundaries. Used at construction time to set the building's finish
    /// date from today's date + the def's `construction_time`.
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

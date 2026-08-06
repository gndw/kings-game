//! The date that walks the calendar. An ECS resource, seeded in `main` from
//! [`crate::resources::calendar::Calendar::start`]; the day-by-day rollover
//! lives in [`crate::updates::advance_date`].

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

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

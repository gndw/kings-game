//! The date that walks the calendar. An ECS resource, seeded to [`Date::START`]
//! in `main`; the day-by-day rollover lives in [`crate::updates::tick`].

use bevy::prelude::Resource;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    /// How many ticks have elapsed since [`START`].
    pub tick_count: u64,
}

impl Date {
    /// When a new game opens. 1066 — the year of the Conquest. Registered as the
    /// starting [`Date`] resource in `main`.
    pub const START: Date = Date {
        year: 1066,
        month: 1,
        day: 1,
        tick_count: 0,
    };

    pub fn is_month_start(&self) -> bool {
        self.day == 1
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

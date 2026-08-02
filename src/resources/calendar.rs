//! The calendar: how long a month and a year are. Comes from `calendar.ron`,
//! so a mod can run ten-day months or a five-month year without a rebuild. An
//! ECS resource, seeded from content in `main`.

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

/// ponytail: every month the same length, no leap days. A real calendar buys
/// nothing here and costs every date calculation in the game — but the lengths
/// themselves are data, so a mod can pick its own.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Resource)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub days_per_month: u8,
    pub months_per_year: u8,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            days_per_month: 30,
            months_per_year: 12,
        }
    }
}

impl Calendar {
    pub fn days_per_year(&self) -> u32 {
        u32::from(self.days_per_month) * u32::from(self.months_per_year)
    }

    /// A zero-length month or year would make the rollover spin forever without
    /// ever rolling over, so this is checked before the game starts.
    pub fn validate(&self) -> Result<()> {
        if self.days_per_month == 0 || self.months_per_year == 0 {
            bail!(
                "calendar needs at least one day per month and one month per year, got {} and {}",
                self.days_per_month,
                self.months_per_year
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_length_calendar_is_rejected() {
        assert!(Calendar::default().validate().is_ok());
        for bad in [(0, 12), (30, 0)] {
            let cal = Calendar {
                days_per_month: bad.0,
                months_per_year: bad.1,
            };
            assert!(cal.validate().is_err(), "{cal:?} must not be accepted");
        }
    }
}

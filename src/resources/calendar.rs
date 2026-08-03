//! The calendar: how long a month and a year are, and the clock speeds. Comes
//! from `calendar.ron`, so a mod can run ten-day months or a five-month year
//! without a rebuild. An ECS resource, seeded from content in `main`.

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

/// ponytail: every month the same length, no leap days. A real calendar buys
/// nothing here and costs every date calculation in the game — but the lengths
/// themselves are data, so a mod can pick its own.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Resource)]
#[serde(deny_unknown_fields, default)]
pub struct Calendar {
    pub days_per_month: u8,
    pub months_per_year: u8,
    /// Simulated days per real second. Keys `1`–`4` select an entry directly;
    /// the game starts on the first entry (paused).
    pub speeds: Vec<u32>,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            days_per_month: 30,
            months_per_year: 12,
            speeds: vec![8, 16, 32, 64],
        }
    }
}

impl Calendar {
    pub fn days_per_year(&self) -> u32 {
        u32::from(self.days_per_month) * u32::from(self.months_per_year)
    }

    /// A zero-length month or year would make the rollover spin forever, an
    /// empty speed list leaves nothing to run at, and a speed of 0 stops the
    /// clock — all checked before the game starts.
    pub fn validate(&self) -> Result<()> {
        if self.days_per_month == 0 || self.months_per_year == 0 {
            bail!(
                "calendar needs at least one day per month and one month per year, got {} and {}",
                self.days_per_month,
                self.months_per_year
            );
        }
        match self.speeds.as_slice() {
            [] => bail!("speeds needs at least one entry"),
            s if s.contains(&0) => bail!("a speed of 0 days/second would stop the clock"),
            _ => {}
        }
        Ok(())
    }
}

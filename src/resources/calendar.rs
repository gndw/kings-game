//! The calendar: month/year lengths, clock speeds, the opening date. From `calendar.ron`.

use super::date::Date;
use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

/// Every month the same length, no leap days. Lengths are data so a mod can pick its own.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Resource)]
#[serde(deny_unknown_fields, default)]
pub struct Calendar {
    pub days_per_month: u8,
    pub months_per_year: u8,
    /// Simulated days per real second. Keys `1`–`4` select an entry directly.
    pub speeds: Vec<u32>,
    pub start: Date,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            days_per_month: 30,
            months_per_year: 12,
            speeds: vec![8, 16, 32, 64],
            start: Date::default(),
        }
    }
}

impl Calendar {
    pub fn days_per_year(&self) -> u32 {
        u32::from(self.days_per_month) * u32::from(self.months_per_year)
    }

    /// Human-readable form of `days`: "1 year 6 months 30 days", omitting zero units.
    pub fn format_duration(&self, days: u32) -> String {
        let dpm = u32::from(self.days_per_month);
        let dpy = self.days_per_year();
        let y = days / dpy;
        let rem = days % dpy;
        let m = rem / dpm;
        let d = rem % dpm;
        let mut s = String::new();
        let mut first = true;
        let mut push = |s: &mut String, n: u32, singular: &str| {
            if !first {
                s.push(' ');
            }
            first = false;
            s.push_str(&format!("{} {}{}", n, singular, if n == 1 { "" } else { "s" }));
        };
        if y > 0 {
            push(&mut s, y, "year");
        }
        if m > 0 {
            push(&mut s, m, "month");
        }
        if d > 0 || s.is_empty() {
            push(&mut s, d, "day");
        }
        s
    }

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

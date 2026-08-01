//! The calendar and the date that walks it. Both come from `calendar.ron`, so
//! a mod can run ten-day months or a five-month year without a rebuild.

use anyhow::{Result, bail};
use serde::Deserialize;

/// ponytail: every month the same length, no leap days. A real calendar buys
/// nothing here and costs every date calculation in the game — but the lengths
/// themselves are data, so a mod can pick its own.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
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

    /// A zero-length month or year would make `advance` spin forever without
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Compares before incrementing rather than after: a mod may set
    /// `days_per_month` to 255, and `day + 1` would wrap the `u8` first.
    pub fn advance(&mut self, cal: &Calendar) {
        if self.day < cal.days_per_month {
            self.day += 1;
            return;
        }
        self.day = 1;
        if self.month < cal.months_per_year {
            self.month += 1;
        } else {
            self.month = 1;
            self.year += 1;
        }
    }

    pub fn is_month_start(&self) -> bool {
        self.day == 1
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(cal: Calendar, days: u32) -> Date {
        let mut date = Date {
            year: 1,
            month: 1,
            day: 1,
        };
        for _ in 0..days {
            date.advance(&cal);
        }
        date
    }

    #[test]
    fn a_year_of_days_rolls_over_exactly_once() {
        for cal in [
            Calendar::default(),
            Calendar {
                days_per_month: 10,
                months_per_year: 5,
            },
            // The degenerate-but-legal ends of the range.
            Calendar {
                days_per_month: 1,
                months_per_year: 1,
            },
            Calendar {
                days_per_month: 255,
                months_per_year: 1,
            },
        ] {
            let start = run(cal, 0);
            assert_eq!(run(cal, cal.days_per_year()), Date { year: 2, ..start });
            // One day short is still the last day of the old year.
            let eve = run(cal, cal.days_per_year() - 1);
            assert_eq!(eve.year, 1);
            assert_eq!(eve.month, cal.months_per_year);
            assert_eq!(eve.day, cal.days_per_month);
        }
    }

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

//! One simulated day, scheduled by Bevy's `FixedUpdate` and chained between
//! the yield recompute and the monthly payout. The date and calendar are ECS
//! resources; the rollover logic lives here.

use crate::resources::{calendar::Calendar, date::Date};
use bevy::prelude::*;

/// One simulated day: the tick count bumps and the date advances. Scheduled in
/// `FixedUpdate`, chained with [`crate::updates::yields::recompute_yields`] and
/// [`crate::updates::payout::monthly_payout`] (which pays out the
/// freshly-recomputed yield on month start). `Time<Fixed>`'s timestep is the
/// game speed and Bevy owns the clock.
pub fn tick(mut date: ResMut<Date>, calendar: Res<Calendar>) {
    date.tick_count += 1;
    advance(&mut date, &calendar);
}

/// Advance `date` by one day along `calendar`. Compares before incrementing
/// rather than after: a mod may set `days_per_month` to 255, and `day + 1`
/// would wrap the `u8` first.
pub fn advance(date: &mut Date, calendar: &Calendar) {
    if date.day < calendar.days_per_month {
        date.day += 1;
        return;
    }
    date.day = 1;
    if date.month < calendar.months_per_year {
        date.month += 1;
    } else {
        date.month = 1;
        date.year += 1;
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
            tick_count: 0,
        };
        for _ in 0..days {
            advance(&mut date, &cal);
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
}

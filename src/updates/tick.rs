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

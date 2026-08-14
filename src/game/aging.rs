//! Derive a character's age in years from their date of birth and the current
//! calendar date. The world stores DOBs (see
//! [`crate::ecs::CharacterDateOfBirth`]); any caller that needs to display or
//! reason about age calls through here so the math stays consistent with the
//! calendar the sim is actually running on.

use crate::resources::calendar::Calendar;
use crate::resources::date::Date;

/// Years elapsed between `dob` and `today`, under `calendar`. If `today` does
/// not yet reach `dob` (bad data — the overlay wrote a future birthday, say)
/// the answer clamps to zero rather than wrapping.
///
/// ponytail: ordinal delta divided by days-per-year, not a year-aware
/// subtraction — the calendar may carry any month/year length a mod picks, so
/// "the same year, minus one" isn't portable. The result is accurate to within
/// a year; sub-year granularity is not displayed anywhere, and adding it back
/// in later would still route through this helper.
pub fn age(dob: &Date, today: &Date, calendar: &Calendar) -> u32 {
    let delta = today.ordinal(calendar) - dob.ordinal(calendar);
    if delta <= 0 {
        return 0;
    }
    (delta as u64 / u64::from(calendar.days_per_year())) as u32
}

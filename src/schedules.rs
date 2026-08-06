//! Custom ECS schedules, beyond Bevy's built-in `Startup`/`Update`/`FixedUpdate`.

use bevy::ecs::schedule::ScheduleLabel;

/// Runs once per simulated day, after [`crate::updates::advance_date::advance`]
/// bumps the date. Holds the per-day building-completion check
/// ([`crate::updates::construction::construction`]); the schedule exists so
/// any future "things that happen daily" code path has one place to register
/// rather than chaining systems onto `FixedUpdate`.
#[derive(ScheduleLabel, Hash, PartialEq, Eq, Clone, Debug)]
pub struct OnDay;

/// Runs once per simulated month, on the day the date rolls back to 1. Fired
/// from [`crate::updates::advance_date::advance`]; holds the monthly economy,
/// currently the tax [`crate::updates::payout::payout`].
#[derive(ScheduleLabel, Hash, PartialEq, Eq, Clone, Debug)]
pub struct OnMonth;

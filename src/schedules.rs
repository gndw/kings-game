//! Custom ECS schedules, beyond Bevy's built-in `Startup`/`Update`/`FixedUpdate`.

use bevy::ecs::schedule::ScheduleLabel;

/// Runs once per simulated month, on the day the date rolls back to 1. Fired
/// from [`crate::updates::advance_date::advance`]; holds the monthly economy,
/// currently the tax [`crate::updates::payout::payout`].
#[derive(ScheduleLabel, Hash, PartialEq, Eq, Clone, Debug)]
pub struct OnMonth;

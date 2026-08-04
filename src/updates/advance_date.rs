//! One simulated day, scheduled by Bevy's `FixedUpdate`. The date and calendar are ECS
//! resources; the rollover logic lives here.

use crate::resources::{calendar::Calendar, date::Date};
use crate::schedules::OnMonth;
use crate::scripting::{OnDay, OnMonth as ScriptOnMonth};
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;

/// One simulated day: the tick count bumps and the date advances. On the day
/// the date rolls back to 1 (a month boundary) it runs the [`OnMonth`]
/// schedule, which holds the monthly economy,
/// currently the tax [`crate::updates::payout::payout`]. Exclusive so it can `run_schedule`,
/// which needs `&mut World`.
pub fn advance(world: &mut World) {
    let (days_per_month, months_per_year) = {
        let calendar = world.resource::<Calendar>();
        (calendar.days_per_month, calendar.months_per_year)
    };
    let month_rolled = {
        let mut date = world.resource_mut::<Date>();
        date.tick_count += 1;
        if date.day < days_per_month {
            date.day += 1;
            false
        } else {
            date.day = 1;
            if date.month < months_per_year {
                date.month += 1;
            } else {
                date.month = 1;
                date.year += 1;
            }
            true
        }
    };

    // Fire the daily script callback so mods can react to each simulated day.
    world
        .resource_mut::<Messages<ScriptCallbackEvent>>()
        .write(ScriptCallbackEvent::new_for_all_scripts(OnDay, vec![]));

    if month_rolled {
        world.run_schedule(OnMonth);
        // Fire the monthly script callback after the built-in OnMonth systems
        // (payout etc.) have run, so mods see the post-payout state.
        world.resource_mut::<Messages<ScriptCallbackEvent>>().write(
            ScriptCallbackEvent::new_for_all_scripts(ScriptOnMonth, vec![]),
        );
    }
}

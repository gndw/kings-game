//! Memory lifecycle: daily sweep of expired memories.
//!
//! Each Memory carries a `MemoryUntilDate` set at creation. Once the world's
//! date has passed it, the memory no longer contributes to opinion (and would
//! just accumulate as dead data), so this system despawns it.
//!
//! Runs on the [`crate::schedules::OnDay`] schedule — every simulated day,
//! right after `advancing_date::tick` has rolled the date forward.

use crate::ecs::character::{Memory, MemoryUntilDate};
use crate::resources::date::Date;
use bevy::prelude::*;

pub fn on_day(
    today: Res<Date>,
    memories: Query<(Entity, &MemoryUntilDate), With<Memory>>,
    mut commands: Commands,
) {
    for (memory_e, until) in &memories {
        if until.0 <= *today {
            commands.entity(memory_e).despawn();
        }
    }
}
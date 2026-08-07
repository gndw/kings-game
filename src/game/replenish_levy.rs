//! Monthly levy replenishment: every ACTIVE building's `BuildingLevy` pool
//! grows by its def's `levy_rate`, capped at the def's `levy`.
//!
//! Runs on the `OnMonth` schedule from [`crate::game::advance_date::advance`],
//! so a building with `levy_rate: 1` and `levy: 50` reaches a full pool in
//! 50 in-game months after the previous raise. The pool keeps filling while
//! an army is in the field (`BuildingIsRaised = true`); the next raise finds
//! however much was replenished since, plus whatever pool the dismissed
//! army left behind.
//!
//! ponytail: two passes (collect, mutate) to dodge "mut-during-iter" pain.
//! Building roster is bounded by the number of standing buildings (small),
//! and the per-building mutation is O(1) anyway, so the second pass is cheap.

use crate::ecs::{BuildingLevy, BuildingOf, BuildingStatus};
use crate::resources::buildings::BuildingDefs;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

/// Increment every ACTIVE building's `BuildingLevy` by its def's
/// `levy_rate`, capped at the def's `levy`. `BuildingIsRaised` is left
/// alone — buildings under an active army keep producing into their
/// (currently drained) pool; the next raise on that land picks up
/// however much has replenished.
pub fn replenish(world: &mut World) {
    // Pass 1: collect `(entity, def_id)` for every ACTIVE building. The
    // query borrows world mutably, so the def lookup happens in pass 2
    // (immutable borrow) before the mutation in pass 3 reborrows mutably.
    let mut active: Vec<(Entity, String)> = Vec::new();
    {
        let mut q = world.query::<(Entity, &BuildingStatus, &BuildingOf)>();
        for (b_e, status, building_of) in q.iter(world) {
            if *status != BuildingStatus::Active {
                continue;
            }
            active.push((b_e, building_of.0.clone()));
        }
    }

    // Pass 2 + 3: look up defs in a tight scope (drops before the
    // `get_mut`), then mutate. Buildings with `levy_rate == 0` are skipped
    // so we don't walk them for nothing.
    for (b_e, def_id) in active {
        let update = {
            let defs = world.resource::<BuildingDefs>();
            defs.get(&def_id).map(|d| (d.levy_rate, d.levy))
        };
        let Some((rate, cap)) = update else { continue; };
        if rate == 0 {
            continue;
        }
        if let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e) {
            building_levy.0 = (building_levy.0 + rate).min(cap);
        }
    }
}
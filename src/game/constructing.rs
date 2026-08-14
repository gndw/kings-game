//! Per-day construction completion: any building whose status is `BUILDING`
//! and whose `BuildingConstructionDate` has been reached flips to `ACTIVE`.
//!
//! Runs on the `OnDay` schedule from [`crate::game::advancing_date::tick`],
//! so a building placed today with `construction_time = 5` finishes at the
//! start of day 6 (tick already moved the date there).
//!
//! ponytail: two queries (`BUILDING` rows; the transition list) instead of one
//! mutable query + a side list — turns what could be a "mut-during-iter"
//! panic-free but awkward pattern into two straight loops. The transition
//! list is bounded by the number of under-construction buildings, which is
//! tiny.

use crate::ecs::{BuildingConstructionDate, BuildingOf, BuildingOnLand, BuildingStatus};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::events::{BuildingUpdateKind, OnBuildingUpdated};
use bevy::ecs::world::World;

/// Walk every `BUILDING` building whose date has been reached, flip it to
/// `ACTIVE`, drop the now-stale `BuildingConstructionDate`, append a
/// chronicle line, and fire the per-kingdom yield observer so the realm's
/// `CharacterGoldYield` / `CharacterLevy` pick up the new contribution.
pub fn on_day(world: &mut World) {
    let (calendar, today) = {
        let c = world.resource::<Calendar>();
        let d = *world.resource::<Date>();
        (c.clone(), d)
    };
    let today_ord = today.ordinal(&calendar);

    // Pass 1: collect. Every entity with the right status and a finish date
    // that's already passed (or equal to today). Building -> (LandEntity,
    // def_id). The def_id is captured here so pass 2 can name the building
    // in the chronicle line without re-reading the world during the
    // mutation loop.
    let mut ready: Vec<(bevy::ecs::entity::Entity, bevy::ecs::entity::Entity, String)> = Vec::new();
    {
        let mut q = world.query::<(
            bevy::ecs::entity::Entity,
            &BuildingStatus,
            &BuildingConstructionDate,
            &BuildingOnLand,
            &BuildingOf,
        )>();
        for (b_e, status, finish_date, building_on_land, building_of) in q.iter(world) {
            if *status != BuildingStatus::Building {
                continue;
            }
            if today_ord >= finish_date.0.ordinal(&calendar) {
                ready.push((b_e, building_on_land.0, building_of.0.clone()));
            }
        }
    }

    // Pass 2: mutate. Flip the status, drop the construction date, and
    // fire `OnBuildingUpdated` — the chronicle observer writes the
    // "is now in operation" line off the entity, and
    // `game::yielding::on_building_updated` re-sums the realm's yield.
    for (b_e, land_e, _def_id) in ready {
        if let Some(mut status) = world.get_mut::<BuildingStatus>(b_e) {
            *status = BuildingStatus::Active;
        }
        world.entity_mut(b_e).remove::<BuildingConstructionDate>();
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: land_e,
            kind: BuildingUpdateKind::Constructed,
        });
    }
}

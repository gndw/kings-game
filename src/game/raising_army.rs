//! Per-day army-formation tick: every army in
//! [`ArmyStatus::Raising`](crate::ecs::army::ArmyStatus::Raising) accretes
//! up to 20 levy per raised building per day into [`ArmyLevy`], until it
//! reaches [`ArmyMaxLevy`](crate::ecs::army::ArmyMaxLevy), at which point
//! the army flips to [`ArmyStatus::Idle`](crate::ecs::army::ArmyStatus::Idle).
//!
//! Runs on the [`OnDay`](crate::schedules::OnDay) schedule from
//! [`crate::game::advance_date::advance`], so an army raised today accretes
//! its first troops tomorrow. The cap of 20 per building per day is the
//! raise tempo — a small army (e.g. `ArmyMaxLevy = 30` from one
//! `levy: 30` building) finishes in two days; a large one (e.g. 200 across
//! ten buildings at full pools) takes a day.
//!
//! ponytail: per-army two-pass (snapshot data, then mutate) so the
//! mutation loop can flip `ArmyStatus` and `ArmyLevy` without fighting
//! a borrowed query. Army roster is bounded by the player's kingdoms;
//! the per-army walk is O(buildings on its land) which is tiny.
//!
//! Building selection is "ACTIVE + `BuildingIsRaised = true`", mirroring
//! how [`crate::commands::raise_army`] flags them. The pool is drained
//! here (the raise command leaves it intact — the formation tick is the
//! one that actually moves troops into the army), capped at the
//! per-day 20 and the remaining room in `ArmyMaxLevy`. Once an army
//! reaches its cap the buildings keep the flag set but the pool stops
//! being drained; [`crate::commands::dismiss_army::dismiss`] is the one
//! that clears `BuildingIsRaised` (via `distribute_levy_back`).
//!
//! ponytail: an army whose `ArmyMaxLevy` exceeds the sum of its raised
//! buildings' pools will never reach `ArmyMaxLevy` and stay `Raising`
//! forever (every per-day take is 0 because every pool is empty). Can't
//! happen with the current raise command — `ArmyMaxLevy` is computed
//! as that same sum at raise time — but a torn world or a future
//! code path that mutates the pool mid-formation could leave it stuck.
//! Worth a `ponytail:` ceiling comment, not worth a fix yet.

use crate::ecs::army::{ArmyLevy, ArmyMaxLevy, ArmyOnLand, ArmyStatus};
use crate::ecs::building::{BuildingIsRaised, BuildingLevy, BuildingStatus};
use crate::ecs::land::LandHasBuildings;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

/// How much levy a single raised building contributes to its army each
/// day. Multiplied by the number of raised buildings on the land, this is
/// the army's per-day accretion rate; the per-army cap (`ArmyMaxLevy`)
/// cuts it short once the army is full.
const PER_BUILDING_DAILY_LEVY: u32 = 20;

/// Walk every army in `Raising` status once. For each, take up to
/// [`PER_BUILDING_DAILY_LEVY`] from every ACTIVE, `BuildingIsRaised = true`
/// building on its land, adding the total to `ArmyLevy`. Once
/// `ArmyLevy >= ArmyMaxLevy`, flip the army to `Idle`. The `>=` (not
/// `==`) is defensive: the per-day 20 cap and per-army room cap mean
/// the last step always lands on equality, but a torn world with
/// `ArmyMaxLevy == 0` should still flip.
pub fn on_day(world: &mut World) {
    // Pass 1: snapshot raising armies. Iterating `With<ArmyStatus>`
    // while mutating the same set would borrow-conflict.
    let raising_armies: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<ArmyStatus>>();
        q.iter(world)
            .filter(|army_e| {
                world
                    .get::<ArmyStatus>(*army_e)
                    .map(|status| *status == ArmyStatus::Raising)
                    .unwrap_or(false)
            })
            .collect()
    };

    for army_e in raising_armies {
        let Some(army_on_land) = world.get::<ArmyOnLand>(army_e).copied() else {
            continue;
        };
        let land_e = army_on_land.0;

        let max_levy = world
            .get::<ArmyMaxLevy>(army_e)
            .map(|army_max_levy| army_max_levy.0)
            .unwrap_or(0);
        let current_levy = world
            .get::<ArmyLevy>(army_e)
            .map(|army_levy| army_levy.0)
            .unwrap_or(0);

        // Already full (e.g. `ArmyMaxLevy == 0`): skip the walk and
        // flip the status. A `Raising` army whose target is 0 has
        // nowhere to accrete to — Idle is the right answer.
        if max_levy == 0 || current_levy >= max_levy {
            if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
                *army_status = ArmyStatus::Idle;
            }
            continue;
        }

        // Snapshot the buildings on the land. Filter to ACTIVE +
        // `BuildingIsRaised = true` so we only take from buildings the
        // raise command committed to this army (defensive against a
        // building constructed mid-formation — it isn't flagged and
        // therefore doesn't contribute).
        let building_entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
            Some(land_has_buildings) => land_has_buildings.iter().collect(),
            None => Vec::new(),
        };

        let mut army_full = false;
        for b_e in building_entities {
            // Read cap fresh inside the loop — earlier iterations may
            // have moved `ArmyLevy` closer to `ArmyMaxLevy`. Cheaper
            // than maintaining a local mirror and tracking borrows.
            let current_levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            let max_levy = world
                .get::<ArmyMaxLevy>(army_e)
                .map(|army_max_levy| army_max_levy.0)
                .unwrap_or(0);
            if current_levy >= max_levy {
                army_full = true;
                break;
            }

            let active = world
                .get::<BuildingStatus>(b_e)
                .map(|status| *status == BuildingStatus::Active)
                .unwrap_or(false);
            if !active {
                continue;
            }
            let is_raised = world
                .get::<BuildingIsRaised>(b_e)
                .map(|building_is_raised| building_is_raised.0)
                .unwrap_or(false);
            if !is_raised {
                continue;
            }
            let pool = world
                .get::<BuildingLevy>(b_e)
                .map(|building_levy| building_levy.0)
                .unwrap_or(0);
            if pool == 0 {
                continue;
            }

            // Take up to the per-building daily cap, the remaining room
            // in `ArmyMaxLevy`, and the building's pool — whichever is
            // smallest. `as u64` aligns the arithmetic with the army's
            // `u64` levy without an intermediate widening.
            let remaining_army_room = (max_levy - current_levy) as u32;
            let take = pool
                .min(PER_BUILDING_DAILY_LEVY)
                .min(remaining_army_room);
            if take == 0 {
                continue;
            }

            // Drain the building's pool, add to the army. Drop both
            // `get_mut` borrows before the next iteration.
            if let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e) {
                building_levy.0 -= take;
            }
            if let Some(mut army_levy) = world.get_mut::<ArmyLevy>(army_e) {
                army_levy.0 += take as u64;
            }
        }

        // Final flip if the inner loop filled the army (the break)
        // or the post-mutation `ArmyLevy` reached the cap.
        let current_levy = world
            .get::<ArmyLevy>(army_e)
            .map(|army_levy| army_levy.0)
            .unwrap_or(0);
        let max_levy = world
            .get::<ArmyMaxLevy>(army_e)
            .map(|army_max_levy| army_max_levy.0)
            .unwrap_or(0);
        if army_full || current_levy >= max_levy {
            if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
                *army_status = ArmyStatus::Idle;
            }
        }
    }
}
//! Per-day marching tick: advance every army through its queued marchings.
//!
//! Each marching is one road (see
//! [`MarchingOnRoad`](crate::ecs::marching::MarchingOnRoad)), so "advance"
//! means walk the army along the chain of marchings the
//! [`MarchingOrder`](crate::commands::marching::MarchingOrder) command
//! traced through the road network — one hop at a time, each costing that
//! road's [`RoadDistanceDays`](crate::ecs::road::RoadDistanceDays).
//!
//! Runs on the `OnDay` schedule from
//! [`crate::game::advance_date::advance`], so each simulated day gets one
//! pass. Each pass:
//!
//! 1. **Idle → Marching.** For each `ArmyStatus::Idle` army, find the first
//!    `MarchingStatus::Scheduled` marching in `ArmyHasMarching` whose
//!    `MarchingFromLand` matches the army's current land. Activate it:
//!    `MarchingStatus = OnRoute`, `MarchingBeginDate = today`,
//!    `MarchingArrivedDate = today + the road's `RoadDistanceDays``,
//!    `ArmyStatus = Marching`,
//!    `ArmyMarching = this marching`. The "first" rule matters when the
//!    player queues multiple marchings on the same source land — the
//!    earliest insertion order wins (the `RelationshipTarget` Vec preserves
//!    order).
//! 2. **Marching → arrived.** For each `ArmyStatus::Marching` army, today
//!    ≥ arrived date → move the army's `ArmyOnLand` to the marching's
//!    target land — the far end of the road it walked — then either
//!    activate the next scheduled marching (the next road of the route,
//!    now that the army stands on its source land) or return the army to
//!    `Idle` and despawn the finished marching. If today < arrived date,
//!    the army is still mid-march and the tick does nothing.
//!
//! ponytail: two passes (snapshot armies, then process one at a time) so
//! mutating `ArmyOnLand` / `ArmyStatus` / `ArmyMarching` inside the loop
//! doesn't fight a borrowed query. The army roster is small (≤ a few per
//! kingdom) so the per-army work is O(1) apart from the relationship
//! walks.

use crate::commands::core::note;
use crate::ecs::army::{ArmyHasMarching, ArmyMarching, ArmyOnLand, ArmyStatus};
use crate::ecs::marching::{
    MarchingArrivedDate, MarchingBeginDate, MarchingFromLand, MarchingOnRoad, MarchingStatus,
    MarchingToLand,
};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::LandName;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

/// How many days marching `road_e` takes: its [`RoadDistanceDays`]. `None`
/// only in a torn world — every road is authored with a `distance_days`
/// ([`validate`](crate::content::validate) rejects a missing or zero one) and
/// [`populate`](crate::ecs::populate) gives every road entity the component,
/// so a road without a cost is a bug, not a data case. Callers refuse to move
/// an army rather than invent a duration.
///
/// The one place the per-road duration is resolved — [`activate`] uses it for
/// the arrived date and
/// [`MarchingOrder`](crate::commands::marching::MarchingOrder) uses it to
/// total a route, so the number the player is quoted is the number the tick
/// then charges.
pub fn road_days(world: &World, road_e: Entity) -> Option<u32> {
    world
        .get::<RoadDistanceDays>(road_e)
        .map(|road_distance_days| road_distance_days.0)
}

/// Walk every army once: idle armies get their first matching scheduled
/// marching activated; marching armies that have arrived get moved to the
/// target land and either chain into the next marching or return to Idle.
pub fn tick(world: &mut World) {
    let (calendar, today) = {
        let c = world.resource::<Calendar>();
        let d = *world.resource::<Date>();
        (c.clone(), d)
    };
    let today_ord = today.ordinal(&calendar);

    // Pass 1: snapshot the army roster. We can't iterate `With<ArmyStatus>`
    // while mutating the same set, so collect the entities first.
    let armies: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<ArmyStatus>>();
        q.iter(world).collect()
    };

    for army_e in armies {
        let status = world
            .get::<ArmyStatus>(army_e)
            .copied()
            .unwrap_or(ArmyStatus::Idle);
        let current_land = world.get::<ArmyOnLand>(army_e).map(|army_on_land| army_on_land.0);
        let current_marching = world
            .get::<ArmyMarching>(army_e)
            .map(|army_marching| army_marching.0);

        match status {
            ArmyStatus::Idle => {
                let Some(land) = current_land else { continue };
                let Some(next_marching) =
                    find_scheduled_matching_from(world, army_e, land)
                else {
                    continue;
                };
                // A `false` here leaves the army Idle and the marching
                // Scheduled — nothing to unwind, and the next tick retries.
                activate(world, army_e, next_marching, today, &calendar);
            }
            ArmyStatus::Marching => {
                let Some(marching_e) = current_marching else { continue };
                let arrived = world
                    .get::<MarchingArrivedDate>(marching_e)
                    .and_then(|d| d.0);
                let Some(arrived) = arrived else { continue };
                if today_ord < arrived.ordinal(&calendar) {
                    continue;
                }

                // Arrived (or late). Move the army to the marching's target
                // land, then look up the next scheduled marching on the new
                // land.
                let Some(target_e) = world
                    .get::<MarchingToLand>(marching_e)
                    .map(|marching_to_land| marching_to_land.0)
                else {
                    continue;
                };
                let from_name = world
                    .get::<LandName>(current_land.unwrap_or(target_e))
                    .map(|land_name| land_name.0.clone())
                    .unwrap_or_else(|| "?".into());
                let to_name = world
                    .get::<LandName>(target_e)
                    .map(|land_name| land_name.0.clone())
                    .unwrap_or_else(|| "?".into());

                // `ArmyOnLand` is a Bevy relationship, whose `Component`
                // impl is `Immutable` by the `Relationship` bound. To move
                // the army we drop the relationship on the old land and
                // re-insert it on the target — Bevy's relationship hooks
                // maintain `LandHasArmies` on both sides in the same call.
                // `entity_mut` returns a fresh handle so the borrow checker
                // is happy with the subsequent reads.
                if world.get::<ArmyOnLand>(army_e).map(|a| a.0) != Some(target_e) {
                    world.entity_mut(army_e).insert(ArmyOnLand(target_e));
                }

                match find_scheduled_matching_from(world, army_e, target_e) {
                    Some(next_marching) => {
                        // Despawn the finished marching, then activate the
                        // next one. Despawning first so the army's
                        // `ArmyHasMarching` shrinks and the next activation
                        // sees authoritative queue state.
                        world.despawn(marching_e);
                        if activate(world, army_e, next_marching, today, &calendar) {
                            note(
                                world,
                                format!("army arrived at {to_name} (continuing march)"),
                            );
                        } else {
                            // The next road has no duration to march for.
                            // The finished marching is already gone, so the
                            // army must stand down here or it would be left
                            // Marching against a despawned entity.
                            stand_down(world, army_e);
                            note(
                                world,
                                format!("army arrived at {to_name} from {from_name} (idle)"),
                            );
                        }
                    }
                    None => {
                        // Queue empty. Return the army to Idle, drop
                        // `ArmyMarching`, and despawn the finished marching.
                        stand_down(world, army_e);
                        world.despawn(marching_e);
                        note(
                            world,
                            format!("army arrived at {to_name} from {from_name} (idle)"),
                        );
                    }
                }
            }
        }
    }
}

/// The first scheduled marching in `army_e`'s queue whose `MarchingFromLand`
/// matches `on_land`. `None` when the army has no matching scheduled
/// marching (the army stays idle, or finishes its current marching and
/// returns to idle). Reads via `world::get` so it stays `&World`-safe.
fn find_scheduled_matching_from(world: &World, army_e: Entity, on_land: Entity) -> Option<Entity> {
    let army_has_marching = world.get::<ArmyHasMarching>(army_e)?;
    army_has_marching.iter().find_map(|m_e| {
        let status = world.get::<MarchingStatus>(m_e)?;
        if *status != MarchingStatus::Scheduled {
            return None;
        }
        let from = world.get::<MarchingFromLand>(m_e)?;
        if from.0 != on_land {
            return None;
        }
        Some(m_e)
    })
}

/// Activate `marching_e` for `army_e`: flip the marching to `OnRoute`,
/// populate begin/arrived dates, set the army to `Marching`, and insert
/// `ArmyMarching`. The march runs from today to today + the
/// [`RoadDistanceDays`] of the road in the marching's `MarchingOnRoad`, so a
/// long road costs more than a short one. The "begin on where the army land
/// is" check was done by the caller (`find_scheduled_matching_from`).
///
/// `false` when the road's duration can't be resolved (see [`road_days`] — a
/// torn world, not a data case): the marching is left `Scheduled` and
/// untouched rather than given an invented arrival date. Callers must not
/// leave the army `Marching` on a `false`.
fn activate(
    world: &mut World,
    army_e: Entity,
    marching_e: Entity,
    today: Date,
    calendar: &Calendar,
) -> bool {
    let Some(days) = world
        .get::<MarchingOnRoad>(marching_e)
        .and_then(|marching_on_road| road_days(world, marching_on_road.0))
    else {
        return false;
    };
    let arrived = today.after_days(days, calendar);
    if let Some(mut marching_status) = world.get_mut::<MarchingStatus>(marching_e) {
        *marching_status = MarchingStatus::OnRoute;
    }
    if let Some(mut marching_begin_date) = world.get_mut::<MarchingBeginDate>(marching_e) {
        marching_begin_date.0 = Some(today);
    }
    if let Some(mut marching_arrived_date) = world.get_mut::<MarchingArrivedDate>(marching_e) {
        marching_arrived_date.0 = Some(arrived);
    }
    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Marching;
    }
    world.entity_mut(army_e).insert(ArmyMarching(marching_e));
    true
}

/// Put `army_e` back to `Idle` and drop its `ArmyMarching` pointer — the end
/// of a route, and the only safe answer when the next marching can't be
/// activated (leaving the army `Marching` with a stale `ArmyMarching` would
/// freeze it: the tick would look up a despawned marching's arrived date
/// every day and skip).
fn stand_down(world: &mut World, army_e: Entity) {
    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Idle;
    }
    world.entity_mut(army_e).remove::<ArmyMarching>();
}

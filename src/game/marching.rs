//! Per-day marching tick: walk the army's queued marchings one road at a time.

use crate::ecs::army::{ArmyHasMarching, ArmyMarching, ArmyOnLand, ArmyStatus};
use crate::ecs::marching::{
    MarchingArrivedDate, MarchingBeginDate, MarchingFromLand, MarchingOnRoad, MarchingStatus,
    MarchingToLand,
};
use crate::ecs::road::RoadDistanceDays;
use crate::events::OnArmyArrived;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

/// How many days marching `road_e` takes. `None` in a torn world — every road
/// is authored with a `distance_days`, so callers refuse to move rather than
/// invent a duration.
pub fn road_days(world: &World, road_e: Entity) -> Option<u32> {
    world
        .get::<RoadDistanceDays>(road_e)
        .map(|road_distance_days| road_distance_days.0)
}

/// Walk every army once: idle armies get their first matching scheduled
/// marching activated; marching armies that have arrived get moved onto the
/// target land and either chain or stand down.
pub fn tick(world: &mut World) {
    let (calendar, today) = {
        let c = world.resource::<Calendar>();
        let d = *world.resource::<Date>();
        (c.clone(), d)
    };
    let today_ord = today.ordinal(&calendar);

    // Snapshot the army roster so we can mutate during iteration.
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
                activate(world, army_e, next_marching, today, &calendar);
            }
            // `Raising` armies are owned by `game::raising_army::on_day` until
            // the formation tick fills them and flips them to `Idle`.
            ArmyStatus::Raising => {}
            ArmyStatus::Marching => {
                let Some(marching_e) = current_marching else { continue };
                let arrived = world
                    .get::<MarchingArrivedDate>(marching_e)
                    .and_then(|d| d.0);
                let Some(arrived) = arrived else { continue };
                if today_ord < arrived.ordinal(&calendar) {
                    continue;
                }

                let Some(target_e) = world
                    .get::<MarchingToLand>(marching_e)
                    .map(|marching_to_land| marching_to_land.0)
                else {
                    continue;
                };
                let from_e = current_land.unwrap_or(target_e);

                // `ArmyOnLand` is Immutable; drop and re-insert to move the army.
                if world.get::<ArmyOnLand>(army_e).map(|a| a.0) != Some(target_e) {
                    world.entity_mut(army_e).insert(ArmyOnLand(target_e));
                }

                let continuing = match find_scheduled_matching_from(world, army_e, target_e) {
                    Some(next_marching) => {
                        world.despawn(marching_e);
                        if activate(world, army_e, next_marching, today, &calendar) {
                            true
                        } else {
                            // The next road has no duration — the finished marching
                            // is gone, so the army must stand down here.
                            stand_down(world, army_e);
                            false
                        }
                    }
                    None => {
                        stand_down(world, army_e);
                        world.despawn(marching_e);
                        false
                    }
                };

                world.trigger(OnArmyArrived {
                    army: army_e,
                    from: from_e,
                    to: target_e,
                    continuing,
                });
            }
            // Sieging armies are owned by `game::besieging::tick`.
            ArmyStatus::Sieging => {}
        }
    }
}

/// The first scheduled marching in `army_e`'s queue whose `MarchingFromLand`
/// matches `on_land`. `None` when no match (army stays idle / finishes its marching).
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

/// Activate `marching_e` for `army_e`. `false` when the road's duration can't
/// be resolved — the marching is left `Scheduled` and untouched.
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

/// Put `army_e` back to `Idle` and drop its `ArmyMarching` pointer.
fn stand_down(world: &mut World, army_e: Entity) {
    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Idle;
    }
    world.entity_mut(army_e).remove::<ArmyMarching>();
}

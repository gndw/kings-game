//! Per-day army-formation tick: every `ArmyStatus::Raising` army accretes
//! up to 20 levy per raised building per day until it reaches `ArmyMaxLevy`,
//! then flips to `Idle`.

use crate::ecs::army::{ArmyLevy, ArmyMaxLevy, ArmyOnLand, ArmyStatus};
use crate::ecs::building::{BuildingIsRaised, BuildingLevy, BuildingStatus};
use crate::ecs::land::LandHasBuildings;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

/// Per-building daily cap on the levy drain. Multiplied by the number of
/// raised buildings, this is the army's per-day accretion rate.
const PER_BUILDING_DAILY_LEVY: u32 = 20;

/// Walk every army in `Raising` status. For each, take up to `PER_BUILDING_DAILY_LEVY`
/// from every ACTIVE `BuildingIsRaised` building on its land, add to `ArmyLevy`,
/// flip to `Idle` once full.
pub fn on_day(world: &mut World) {
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

        // Already full (or `ArmyMaxLevy == 0`): skip the walk and flip to Idle.
        if max_levy == 0 || current_levy >= max_levy {
            if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
                *army_status = ArmyStatus::Idle;
            }
            continue;
        }

        let building_entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
            Some(land_has_buildings) => land_has_buildings.iter().collect(),
            None => Vec::new(),
        };

        let mut army_full = false;
        for b_e in building_entities {
            // Read fresh inside the loop — earlier iterations may have moved
            // `ArmyLevy` closer to `ArmyMaxLevy`.
            let current_levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);
            let max_levy = world.get::<ArmyMaxLevy>(army_e).map(|x| x.0).unwrap_or(0);
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
            let pool = world.get::<BuildingLevy>(b_e).map(|x| x.0).unwrap_or(0);
            if pool == 0 {
                continue;
            }

            let remaining_army_room = (max_levy - current_levy) as u32;
            let take = pool.min(PER_BUILDING_DAILY_LEVY).min(remaining_army_room);
            if take == 0 {
                continue;
            }

            if let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e) {
                building_levy.0 -= take;
            }
            if let Some(mut army_levy) = world.get_mut::<ArmyLevy>(army_e) {
                army_levy.0 += take as u64;
            }
        }

        let current_levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);
        let max_levy = world.get::<ArmyMaxLevy>(army_e).map(|x| x.0).unwrap_or(0);
        if army_full || current_levy >= max_levy {
            if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
                *army_status = ArmyStatus::Idle;
            }
        }
    }
}

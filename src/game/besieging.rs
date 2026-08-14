//! Per-day siege tick: advance progress on each siege's scheduled event and
//! resolve the ones that hit 100%.

use crate::ecs::army::{ArmyControlsLand, ArmyStatus};
use crate::ecs::building::{Building, BuildingLevy, BuildingStatus};
use crate::ecs::land::LandHasBuildings;
use crate::ecs::siege::{Siege, SiegeAttackerArmy, SiegeDefenderLand, SiegeNextEventDate, SiegeProgress};
use crate::events::OnSiegeWon;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

const EVENT_INTERVAL_DAYS: u32 = 10;
const EVENT_PROGRESS_GAIN: u32 = 30;

/// Resolve every siege whose event date is today. One exclusive pass per day.
pub fn tick(world: &mut World) {
    let calendar = world.resource::<Calendar>().clone();
    let today = *world.resource::<Date>();
    let today_ord = today.ordinal(&calendar);

    let sieges: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<Siege>>();
        q.iter(world).collect()
    };

    for siege_e in sieges {
        let next = world
            .get::<SiegeNextEventDate>(siege_e)
            .map(|siege_next_event_date| siege_next_event_date.0);
        let Some(next) = next else { continue };
        if today_ord < next.ordinal(&calendar) {
            continue;
        }

        let army = world
            .get::<SiegeAttackerArmy>(siege_e)
            .map(|siege_attacker_army| siege_attacker_army.0);
        let land = world
            .get::<SiegeDefenderLand>(siege_e)
            .map(|siege_defender_land| siege_defender_land.0);

        let Some(army_e) = army else {
            world.despawn(siege_e);
            continue;
        };
        if world.get_entity(army_e).is_err() {
            world.despawn(siege_e);
            continue;
        }
        let Some(land_e) = land else {
            world.despawn(siege_e);
            continue;
        };
        if world.get_entity(land_e).is_err() {
            world.despawn(siege_e);
            continue;
        }

        let prev = world
            .get::<SiegeProgress>(siege_e)
            .map(|siege_progress| siege_progress.0)
            .unwrap_or(0);
        let new_progress = (prev + EVENT_PROGRESS_GAIN).min(100);

        if new_progress < 100 {
            let next_event = today.after_days(EVENT_INTERVAL_DAYS, &calendar);
            if let Some(mut siege_progress) = world.get_mut::<SiegeProgress>(siege_e) {
                siege_progress.0 = new_progress;
            }
            if let Some(mut siege_next_event_date) =
                world.get_mut::<SiegeNextEventDate>(siege_e)
            {
                siege_next_event_date.0 = next_event;
            }
            continue;
        }

        // Won! Insert `ArmyControlsLand` on the army before despawning the siege
        // so the relationship hook sees both ends.
        world.entity_mut(army_e).insert(ArmyControlsLand(land_e));

        // Set every standing building on the land to `Inactive` and drain its levy.
        // Buildings stay `Inactive` until the player enforces the `Take` demand.
        let building_entities: Vec<Entity> = world
            .get::<LandHasBuildings>(land_e)
            .map(|land_has_buildings| land_has_buildings.iter().collect())
            .unwrap_or_default();
        for b_e in building_entities {
            if world.get::<Building>(b_e).is_none() {
                continue;
            }
            if let Some(mut building_status) = world.get_mut::<BuildingStatus>(b_e) {
                *building_status = BuildingStatus::Inactive;
            }
            if let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e) {
                building_levy.0 = 0;
            }
        }

        if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
            *army_status = ArmyStatus::Idle;
        }

        world.trigger(OnSiegeWon {
            army: army_e,
            land: land_e,
        });

        world.despawn(siege_e);
    }
}

//! Per-day siege tick: advance every siege's progress on its scheduled event
//! and resolve the ones that hit 100%.
//!
//! Runs on the [`OnDay`](crate::schedules::OnDay) schedule from
//! [`crate::game::advance_date::advance`]. Each pass:
//!
//! 1. Snapshot every siege entity (can't iterate `With<Siege>` while mutating
//!    the same set in the loop body).
//! 2. For each, compare today's ordinal to `SiegeNextEventDate`. If today's
//!    not the event day, skip — the siege waits.
//! 3. If it *is* the event day, add 30 to `SiegeProgress` (capped at 100)
//!    and advance the next-event date by 10 days.
//! 4. If the new progress is `>= 100`, resolve the siege:
//!    - Insert [`ArmyControlsLand`](crate::ecs::ArmyControlsLand) on the
//!      attacking army. Bevy's relationship hook fills
//!      [`LandControlledByArmy`](crate::ecs::LandControlledByArmy) on the
//!      land.
//!    - Set every standing building on the land to
//!      [`BuildingStatus::Inactive`](crate::ecs::BuildingStatus::Inactive) —
//!      the conquest chokes the realm's economy until the defender
//!      reclaims.
//!    - Flip the army's `ArmyStatus` back to `Idle`.
//!    - Despawn the siege entity.
//! 5. If the attacker army has been despawned (the player dismissed it
//!    mid-siege), drop the siege too — leaving a siege pointing at a
//!    despawned entity would freeze the tick forever on its next event day.
//!
//! The tick is exclusive (takes `&mut World`) for the same reason the
//! marching tick is — it mixes component mutation with resource reads.
//! ponytail: two passes (snapshot sieges, then process) avoid fighting
//! borrowed queries in the loop body. The siege count is bounded by the
//! player's army count, so the walk is cheap.

use crate::commands::core::note;
use crate::ecs::army::{ArmyControlsLand, ArmyStatus};
use crate::ecs::building::{Building, BuildingOnLand, BuildingStatus};
use crate::ecs::land::{LandHasBuildings, LandName};
use crate::ecs::siege::{Siege, SiegeAttackerArmy, SiegeDefenderLand, SiegeNextEventDate, SiegeProgress};
use crate::ecs::ArmyName;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, With};

/// How many days between siege events.
const EVENT_INTERVAL_DAYS: u32 = 10;
/// How much progress each scheduled event adds, capped at 100.
const EVENT_PROGRESS_GAIN: u32 = 30;

/// Resolve every siege whose event date is today. One exclusive pass per day.
pub fn tick(world: &mut World) {
    let calendar = world.resource::<Calendar>().clone();
    let today = *world.resource::<Date>();
    let today_ord = today.ordinal(&calendar);

    // Pass 1: snapshot siege entities. Iterating `With<Siege>` while
    // mutating the same set would borrow-conflict.
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

        // Defensive: if the army was dismissed mid-siege, drop the
        // siege (and the chronicle line so the player sees why it went
        // away). Same for a missing land — shouldn't happen, but a
        // despawning land in a future world would otherwise freeze the
        // tick on this entity forever.
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

        // Advance progress. Read, then drop, then write — same pattern
        // as `marching::tick` so a long pass doesn't fight the borrow
        // checker on edge cases.
        let prev = world
            .get::<SiegeProgress>(siege_e)
            .map(|siege_progress| siege_progress.0)
            .unwrap_or(0);
        let new_progress = (prev + EVENT_PROGRESS_GAIN).min(100);

        if new_progress < 100 {
            // Schedule the next event and write the new progress.
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

        // Won! Insert `ArmyControlsLand` on the army. Bevy's relationship
        // hook fills `LandControlledByArmy` on the land. Do this BEFORE
        // despawning the siege so the relationship is in place while the
        // siege still exists (a hook walking `SiegeDefenderLand` reads
        // both ends). After the insert the siege is despawned; the
        // relationship hook on `SiegeDefenderLand` then prunes the land's
        // `LandHasSiegesUnderAttack`.
        world.entity_mut(army_e).insert(ArmyControlsLand(land_e));

        // Set every standing building on the land to `Inactive`. A
        // future "conquest transfer" would also touch the kingdom
        // link (`LandHeldBy`), but that's the war-resolution piece
        // still TBD — for now the economy is the visible consequence.
        // Snapshot the buildings, drop the borrow, then mutate.
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
        }

        // Army returns to Idle. `Sieging` was set by the `Siege` command;
        // clearing it here lets the marching tick and the palette see a
        // normal army again. `ArmyHasSiege` is auto-pruned by Bevy's
        // relationship hook on `SiegeAttackerArmy` when we despawn the
        // siege below.
        if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
            *army_status = ArmyStatus::Idle;
        }

        // Chronicle the conquest before the siege despawn — the siege
        // entity's id is what `note` would otherwise lose context on.
        let army_label = world
            .get::<ArmyName>(army_e)
            .map(|army_name| army_name.0.clone())
            .unwrap_or_else(|| "Army".to_string());
        let land_label = world
            .get::<LandName>(land_e)
            .map(|land_name| land_name.0.clone())
            .unwrap_or_else(|| "?".into());
        note(
            world,
            format!("{army_label} took {land_label} (siege won)"),
        );

        world.despawn(siege_e);

        // Silence unused-import warnings for items brought in only for
        // the relationship target contracts. (Bevy requires the target
        // component to be defined for the source `#[relationship]` to
        // type-check, but the siege tick doesn't need to read them
        // directly.)
        let _ = std::any::type_name::<BuildingOnLand>();
    }
}

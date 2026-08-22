//! Re-activate buildings on a land whose kingdom was just taken over via a
//! `Take` demand, provided no enemy army still controls the land.
//!
//! The siege tick sets every building on the land to `Inactive` (and
//! drains its [`BuildingLevy`](crate::ecs::BuildingLevy) to `0`) the
//! moment the siege resolves at 100% — that's the visible "land is
//! conquered but not yet yours" state. The buildings stay `Inactive`
//! until the player enforces the [`Take`](crate::ecs::WarDemandType::Take)
//! demand on the war the conquered land sits in
//! ([`crate::commands::enforce_demands::EnforceDemands`]), which swaps
//! the kingdom's Ruler courtier to the player via
//! [`set_ruler`](crate::helper::kingdom_helper::set_ruler). At that
//! moment this observer checks whether an enemy army is still controlling
//! the land; if not, every building flips back to `Active` so the new
//! owner actually owns a working realm.
//!
//! Runs as a Bevy observer for
//! [`OnDemandEnforced`](crate::observers::OnDemandEnforced). Only `Take`
//! triggers a release; new variants on
//! [`WarDemandType`](crate::ecs::WarDemandType) are additive and can opt
//! in here.
//!
//! ponytail: one observer, one enemy check + one status flip per building.
//! No [`OnBuildingUpdated`](crate::observers::OnBuildingUpdated) fired —
//! the yields observer keys off status, and firing it would require
//! threading the new land through `on_building_updated` per building.
//! The existing siege tick doesn't fire it either when flipping to
//! `Inactive`, so staying consistent.
use crate::ecs::{
    ArmyBelongsToKingdom, Building, BuildingStatus, KingdomHold,
    LandControlledByArmy, LandHasBuildings, WarDemandType,
};
use crate::helper::kingdom_helper::kingdom_ruler;
use crate::observers::OnDemandEnforced;
use bevy::ecs::entity::Entity;
use bevy::prelude::*;

/// Observer for [`OnDemandEnforced`] on
/// [`Take`](crate::ecs::WarDemandType::Take). Flips every standing
/// building on the target kingdom's held land back to `Active`, unless
/// an enemy army currently controls the land (in which case the land is
/// contested and the buildings stay `Inactive`).
pub fn on_demand_enforced(
    trigger: On<OnDemandEnforced>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let demand_type = event.demand_type;
    let target_kingdom = event.target;
    commands.queue(move |world: &mut World| {
        if !matches!(demand_type, WarDemandType::Take) {
            return;
        }

        // The new leader of the target kingdom — `enforce_take` swaps the
        // Ruler courtier via `set_ruler` BEFORE firing `OnDemandEnforced`,
        // so the new ruler is already in place when this observer runs.
        let Some(player_e) = kingdom_ruler(world, target_kingdom) else {
            return;
        };
        let Some(KingdomHold(target_land)) = world.get::<KingdomHold>(target_kingdom).copied() else {
            return;
        };

        // Enemy check: if the controlling army (if any) belongs to a
        // kingdom NOT led by the new ruler, the land is still contested —
        // skip. The new ruler's other kingdoms count as friendly
        // (`ArmyBelongsToKingdom` → `kingdom_ruler == player_e`).
        let enemy_holds = world
            .get::<LandControlledByArmy>(target_land)
            .and_then(|lca| world.get::<ArmyBelongsToKingdom>(lca.army()))
            .map(|abtk| abtk.0)
            .map(|army_kingdom| {
                kingdom_ruler(world, army_kingdom)
                    .map(|leader| leader != player_e)
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        if enemy_holds {
            return;
        }

        // Snapshot the building entities, drop the borrow, then flip each
        // status. Two passes dodge "mut during iter" pain on
        // `BuildingStatus`.
        let Some(land_has_buildings) = world.get::<LandHasBuildings>(target_land) else {
            return;
        };
        let building_entities: Vec<Entity> = land_has_buildings.iter().collect();
        let mut building_statuses = world.query::<&mut BuildingStatus>();
        for b_e in building_entities {
            if world.get::<Building>(b_e).is_none() {
                continue;
            }
            if let Ok(mut status) = building_statuses.get_mut(world, b_e) {
                *status = BuildingStatus::Active;
            }
        }
    });
}

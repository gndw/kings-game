//! Re-activate buildings on a land whose kingdom was just taken over via a
//! `Take` demand, provided no enemy army still controls the land.
//!
//! The siege tick sets every building on the land to `Inactive` (and
//! drains its [`BuildingLevy`](crate::ecs::BuildingLevy) to `0`) the
//! moment the siege resolves at 100% — that's the visible "land is
//! conquered but not yet yours" state. The buildings stay `Inactive`
//! until the player enforces the [`Take`](crate::ecs::WarDemandType::Take)
//! demand on the war the conquered land sits in
//! ([`crate::commands::enforce_demands::EnforceDemands`]), which transfers
//! [`KingdomLedBy`](crate::ecs::KingdomLedBy) to the player. At that
//! moment this observer checks whether an enemy army is still controlling
//! the land; if not, every building flips back to `Active` so the new
//! owner actually owns a working realm.
//!
//! Runs as a Bevy observer for
//! [`OnDemandEnforced`](crate::events::OnDemandEnforced). Only `Take`
//! triggers a release; new variants on
//! [`WarDemandType`](crate::ecs::WarDemandType) are additive and can opt
//! in here.
//!
//! ponytail: one observer, one enemy check + one status flip per building.
//! No [`OnBuildingUpdated`](crate::events::OnBuildingUpdated) fired —
//! the yields observer keys off status, and firing it would require
//! threading the new land through `on_building_updated` per building.
//! The existing siege tick doesn't fire it either when flipping to
//! `Inactive`, so staying consistent.
use crate::ecs::{
    ArmyBelongsToKingdom, Building, BuildingStatus, KingdomHold, KingdomLedBy,
    LandControlledByArmy, LandHasBuildings, WarDemandType,
};
use crate::events::OnDemandEnforced;
use bevy::ecs::entity::Entity;
use bevy::prelude::*;

/// Observer for [`OnDemandEnforced`] on
/// [`Take`](crate::ecs::WarDemandType::Take). Flips every standing
/// building on the target kingdom's held land back to `Active`, unless
/// an enemy army currently controls the land (in which case the land is
/// contested and the buildings stay `Inactive`).
pub fn on_demand_enforced(
    trigger: On<OnDemandEnforced>,
    kingdom_holds: Query<&KingdomHold>,
    kingdom_led_by: Query<&KingdomLedBy>,
    land_controlled_by_army: Query<&LandControlledByArmy>,
    army_belongs_to_kingdom: Query<&ArmyBelongsToKingdom>,
    land_has_buildings: Query<&LandHasBuildings>,
    buildings: Query<&Building>,
    mut building_statuses: Query<&mut BuildingStatus>,
) {
    let event = trigger.event();
    if !matches!(event.demand_type, WarDemandType::Take) {
        return;
    }
    let target_kingdom = event.target;

    // The new leader of the target kingdom — `enforce_take` just
    // inserted `KingdomLedBy(actor)` so the player is here. A missing
    // leader means the kingdom is torn (the Take path also failed),
    // so the release can't safely identify "us" — bail.
    let Ok(KingdomLedBy(player_e)) = kingdom_led_by.get(target_kingdom) else {
        return;
    };
    // The target land is the kingdom's held land. `Take`'s gate
    // already required this to resolve; defensive check.
    let Ok(KingdomHold(target_land)) = kingdom_holds.get(target_kingdom) else {
        return;
    };
    let target_land = *target_land;

    // Enemy check: if the controlling army (if any) belongs to a
    // kingdom NOT led by the player, the land is still contested —
    // skip. The player's other kingdoms count as friendly
    // (`ArmyBelongsToKingdom` → `KingdomLedBy == player_e`). Missing
    // controller is treated as "no enemy", which lets the release
    // through; the building panel will still read `Inactive` until
    // this observer flips it.
    let enemy_holds = land_controlled_by_army
        .get(target_land)
        .ok()
        .and_then(|lca| army_belongs_to_kingdom.get(lca.army()).ok())
        .map(|abtk| {
            kingdom_led_by
                .get(abtk.0)
                .ok()
                .map(|KingdomLedBy(leader)| leader != player_e)
                .unwrap_or(true)
        })
        .unwrap_or(false);
    if enemy_holds {
        return;
    }

    // Snapshot the building entities, drop the borrow, then flip each
    // status. Two passes dodge "mut during iter" pain on
    // `BuildingStatus`.
    let Ok(land_has_buildings) = land_has_buildings.get(target_land) else {
        return;
    };
    let building_entities: Vec<Entity> = land_has_buildings.iter().collect();
    for b_e in building_entities {
        if buildings.get(b_e).is_err() {
            continue;
        }
        if let Ok(mut status) = building_statuses.get_mut(b_e) {
            *status = BuildingStatus::Active;
        }
    }
}

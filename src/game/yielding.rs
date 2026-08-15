//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingStatus, CharacterGoldYield, CharacterLeads, CharacterLevy, KingdomHold,
    KingdomLedBy, LandHasBuildings, LandHeldBy,
};
use crate::resources::buildings::BuildingDefs;
use crate::observers::OnBuildingUpdated;
use bevy::prelude::*;

/// Sum a land's buildings into `(gold, levy)`. Inactive and `Building` contribute nothing.
pub fn sum_land_yield(
    land_e: Entity,
    land_has_buildings: &Query<&LandHasBuildings>,
    building_of: &Query<&BuildingOf>,
    building_status: &Query<&BuildingStatus>,
    defs: &BuildingDefs,
) -> (i64, u64) {
    let Ok(land_has_buildings) = land_has_buildings.get(land_e) else {
        return (0, 0);
    };
    let (mut gold, mut levy) = (0i64, 0u64);
    for b_e in land_has_buildings.iter() {
        let Ok(building_of) = building_of.get(b_e) else { continue };
        let active = building_status
            .get(b_e)
            .map(|status| *status == BuildingStatus::Active)
            .unwrap_or(false);
        if !active { continue };
        if let Some(d) = defs.get(&building_of.0) {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
        }
    }
    (gold, levy)
}

/// Recompute every character's `gold_yield` and `levy` from their holdings: a leader's
/// realm summed across every kingdom they lead; everyone else zeroed. Runs in `Startup`.
pub fn recompute_yields(
    mut characters: Query<(
        Option<&CharacterLeads>,
        &mut CharacterGoldYield,
        &mut CharacterLevy,
    )>,
    kingdom_holds: Query<&KingdomHold>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    defs: Res<BuildingDefs>,
) {
    for (character_leads, mut character_gold_yield, mut character_levy) in &mut characters {
        let (mut g, mut l) = (0i64, 0u64);
        if let Some(character_leads) = character_leads {
            for kingdom_e in character_leads.kingdoms() {
                let Ok(kingdom_hold) = kingdom_holds.get(*kingdom_e) else { continue };
                let (dg, dl) = sum_land_yield(
                    kingdom_hold.0,
                    &land_has_buildings,
                    &building_of,
                    &building_status,
                    &defs,
                );
                g += dg;
                l += dl;
            }
        }
        character_gold_yield.0 = g;
        character_levy.0 = l;
    }
}

/// Re-sum every kingdom the affected-land's leader rules and write that one leader's yield + levy.
/// The leader's full realm is re-summed, so any change to one of the leader's lands refreshes all.
pub fn on_building_updated(
    trigger: On<OnBuildingUpdated>,
    game: Option<Res<Game>>,
    land_held_by: Query<&LandHeldBy>,
    kingdom_led_by: Query<&KingdomLedBy>,
    character_leads: Query<&CharacterLeads>,
    mut character_gold_yields: Query<(&mut CharacterGoldYield, &mut CharacterLevy)>,
    kingdom_holds: Query<&KingdomHold>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    defs: Res<BuildingDefs>,
) {
    if game.is_none() {
        return;
    }
    let land_e = trigger.event().land;
    let Ok(land_held_by) = land_held_by.get(land_e) else { return };
    let kingdom_e = land_held_by.kingdom();
    let Ok(&KingdomLedBy(leader_e)) = kingdom_led_by.get(kingdom_e) else { return };
    let Ok((mut character_gold_yield, mut character_levy)) = character_gold_yields.get_mut(leader_e) else {
        return;
    };
    let (mut g, mut l) = (0i64, 0u64);
    if let Ok(character_leads) = character_leads.get(leader_e) {
        for kingdom_e in character_leads.kingdoms() {
            let Ok(kingdom_hold) = kingdom_holds.get(*kingdom_e) else { continue };
            let (dg, dl) = sum_land_yield(
                kingdom_hold.0,
                &land_has_buildings,
                &building_of,
                &building_status,
                &defs,
            );
            g += dg;
            l += dl;
        }
    }
    character_gold_yield.0 = g;
    character_levy.0 = l;
}

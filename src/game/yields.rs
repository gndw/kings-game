//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingStatus, CharacterGoldYield, CharacterLeads, CharacterLevy, KingdomHold,
    KingdomLedBy, LandHasBuildings, LandHeldBy,
};
use crate::resources::buildings::BuildingDefs;
use bevy::prelude::*;

/// Fired by anything that mutates a building's kingdom-graph footprint
/// (construct, destroy, future code paths that move a building or hot-swap
/// its definition). The commands fire this event *after* their structural
/// change settles Bevy's relationship hooks, so the observer's
/// [`sum_land_yield`] walk sees authoritative data. Observer is
/// [`on_building_updated`].
#[derive(Event)]
pub struct OnBuildingUpdated {
    pub building: Entity,
    pub land: Entity,
    /// 1 = constructed, 2 = updated (reserved for future code paths), 3 =
    /// destroyed. Constants [`BUILDING_CONSTRUCTED`], [`BUILDING_UPDATED`],
    /// [`BUILDING_DESTROYED`] in this module.
    pub r#type: u8,
}

pub const BUILDING_CONSTRUCTED: u8 = 1;
pub const BUILDING_UPDATED: u8 = 2;
pub const BUILDING_DESTROYED: u8 = 3;

/// Sum a land's buildings into `(gold, levy)`. Walks
/// `land → LandHasBuildings → BuildingOf → BuildingDefs` once;
/// `gold_profit - gold_upkeep` accumulates into gold, `levy` accumulates into
/// troops. Pure — no character iteration, shared by [`recompute_yields`], the
/// dirty-yield observer, and the map's per-land yield label. Buildings still
/// under construction (`BuildingStatus != ACTIVE`) do **not** contribute —
/// they will, once `construction` flips them.
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
        let Ok(building_of) = building_of.get(b_e) else {
            continue;
        };
        // Inactive and Building contribute nothing to yields.
        let active = building_status
            .get(b_e)
            .map(|status| *status == BuildingStatus::Active)
            .unwrap_or(false);
        if !active {
            continue;
        }
        if let Some(d) = defs.get(&building_of.0) {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
        }
    }
    (gold, levy)
}

/// Recompute every character's `gold_yield` and `levy` from their holdings: a
/// leader's realm summed via [`sum_land_yield`] (the kingdom's held land);
/// everyone else zeroed. Runs in `Startup` so the opening screen already shows
/// what a realm renders. `Option<&CharacterLeads>` walks every character so a
/// non-ruler is zeroed, not left stale. After startup the construct/destroy
/// commands trigger [`OnBuildingUpdated`] for per-realm updates.
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
        let (g, l) = character_leads
            .and_then(|character_leads| kingdom_holds.get(character_leads.kingdom()).ok())
            .map(|kingdom_hold| {
                sum_land_yield(
                    kingdom_hold.0,
                    &land_has_buildings,
                    &building_of,
                    &building_status,
                    &defs,
                )
            })
            .unwrap_or((0, 0));
        character_gold_yield.0 = g;
        character_levy.0 = l;
    }
}

/// Re-sum the kingdom that holds the event's `land` and write that one
/// leader's [`CharacterGoldYield`] and [`CharacterLevy`]. Wired up as the
/// observer for [`OnBuildingUpdated`]; called via
/// `world.trigger(OnBuildingUpdated { building, land, r#type: ... })`
/// straight after the relevant structural change settles the relationship
/// hooks, so `LandHasBuildings` is already authoritative.
pub fn on_building_updated(
    trigger: On<OnBuildingUpdated>,
    game: Option<Res<Game>>,
    land_held_by: Query<&LandHeldBy>,
    kingdom_led_by: Query<&KingdomLedBy>,
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
    let Ok(land_held_by) = land_held_by.get(land_e) else {
        return;
    };
    let kingdom_e = land_held_by.kingdom();
    let Ok(&KingdomLedBy(leader_e)) = kingdom_led_by.get(kingdom_e) else {
        return;
    };
    let Ok((mut character_gold_yield, mut character_levy)) =
        character_gold_yields.get_mut(leader_e)
    else {
        return;
    };
    let Ok(kingdom_hold) = kingdom_holds.get(kingdom_e) else {
        return;
    };
    let (g, l) = sum_land_yield(
        kingdom_hold.0,
        &land_has_buildings,
        &building_of,
        &building_status,
        &defs,
    );
    character_gold_yield.0 = g;
    character_levy.0 = l;
}

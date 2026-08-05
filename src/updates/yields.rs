//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::app::Game;
use crate::ecs::{
    BuildingOf, CharacterGoldYield, CharacterLevy, Holds, HeldBy, Leads, LedBy, BuildingsOn,
};
use crate::resources::buildings::BuildingDefs;
use bevy::prelude::*;

/// Fired by anything that mutates a building's kingdom-graph footprint
/// (construct, destroy, future code paths that move a building or hot-swap
/// its definition). The commands fire this event *after* their structural
/// change settles Bevy's relationship hooks, so the observer's
/// [`sum_kingdom_yield`] walk sees authoritative data. Observer is
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

/// Sum the realm's holdings into `(gold, levy)`. Walks
/// `kingdom → Holds → lands → BuildingsOn → BuildingOf → BuildingDefs` once;
/// `gold_profit - gold_upkeep` accumulates into gold, `levy` accumulates into
/// troops. Pure — no character iteration, shared by [`recompute_yields`] and
/// the dirty-yield observer.
fn sum_kingdom_yield(
    kingdom_e: Entity,
    kingdoms: &Query<&Holds>,
    lands: &Query<&BuildingsOn>,
    buildings: &Query<&BuildingOf>,
    defs: &BuildingDefs,
) -> (i64, u64) {
    let Ok(holds) = kingdoms.get(kingdom_e) else {
        return (0, 0);
    };
    let (mut gold, mut levy) = (0i64, 0u64);
    for land_e in holds.iter() {
        let Ok(on) = lands.get(land_e) else {
            continue;
        };
        for b_e in on.iter() {
            let Ok(of) = buildings.get(b_e) else {
                continue;
            };
            if let Some(d) = defs.get(&of.0) {
                gold += d.gold_profit as i64 - d.gold_upkeep as i64;
                levy += d.levy as u64;
            }
        }
    }
    (gold, levy)
}

/// Recompute every character's `gold_yield` and `levy` from their holdings: a
/// leader's realm summed via [`sum_kingdom_yield`]; everyone else zeroed.
/// Runs in `Startup` so the opening screen already shows what a realm renders.
/// `Option<&Leads>` walks every character so a non-ruler is zeroed, not left
/// stale. After startup the construct/destroy commands trigger
/// [`OnBuildingUpdated`] for per-realm updates.
pub fn recompute_yields(
    mut characters: Query<(Option<&Leads>, &mut CharacterGoldYield, &mut CharacterLevy)>,
    kingdoms: Query<&Holds>,
    lands: Query<&BuildingsOn>,
    buildings: Query<&BuildingOf>,
    defs: Res<BuildingDefs>,
) {
    for (leads, mut gold_yield, mut levy) in &mut characters {
        let (g, l) = leads
            .map(|l| sum_kingdom_yield(l.kingdom(), &kingdoms, &lands, &buildings, &defs))
            .unwrap_or((0, 0));
        gold_yield.0 = g;
        levy.0 = l;
    }
}

/// Re-sum the kingdom that holds the event's `land` and write that one
/// leader's [`CharacterGoldYield`] and [`CharacterLevy`]. Wired up as the
/// observer for [`OnBuildingUpdated`]; called via
/// `world.trigger(OnBuildingUpdated { building, land, r#type: ... })`
/// straight after the relevant structural change settles the relationship
/// hooks, so `BuildingsOn` is already authoritative.
pub fn on_building_updated(
    trigger: On<OnBuildingUpdated>,
    game: Option<Res<Game>>,
    held_by: Query<&HeldBy>,
    led_by: Query<&LedBy>,
    mut chars: Query<(&mut CharacterGoldYield, &mut CharacterLevy)>,
    kingdoms: Query<&Holds>,
    lands: Query<&BuildingsOn>,
    buildings: Query<&BuildingOf>,
    defs: Res<BuildingDefs>,
) {
    if game.is_none() {
        return;
    }
    let land_e = trigger.event().land;
    let Ok(&HeldBy(kingdom_e)) = held_by.get(land_e) else {
        return;
    };
    let Ok(&LedBy(leader_e)) = led_by.get(kingdom_e) else {
        return;
    };
    let Ok((mut gy, mut lv)) = chars.get_mut(leader_e) else {
        return;
    };
    let (g, l) = sum_kingdom_yield(kingdom_e, &kingdoms, &lands, &buildings, &defs);
    gy.0 = g;
    lv.0 = l;
}

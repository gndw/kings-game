//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingStatus, Character, CharacterGoldYield, CharacterLevy, KingdomHold,
    LandHasBuildings, LandHeldBy,
};
use crate::helper::kingdom_helper::{get_character_ruled_kingdoms, get_kingdom_ruler};
use crate::resources::buildings::BuildingDefs;
use crate::observers::OnBuildingUpdated;
use bevy::prelude::*;

/// Sum a land's buildings into `(gold, levy)`. Inactive and `Building` contribute nothing.
/// Reads through `&World` so it works in both param-style and exclusive systems.
/// Reads the `BuildingDefs` resource itself to avoid forcing callers to hold a
/// separate borrow (which would clash with `query.iter_mut`).
pub fn sum_land_yield(
    land_e: Entity,
    world: &World,
) -> (i64, u64) {
    let Some(land_has_buildings) = world.get::<LandHasBuildings>(land_e) else {
        return (0, 0);
    };
    let defs = world.resource::<BuildingDefs>();
    let (mut gold, mut levy) = (0i64, 0u64);
    for b_e in land_has_buildings.iter() {
        let Some(building_of) = world.get::<BuildingOf>(b_e) else { continue };
        let active = world
            .get::<BuildingStatus>(b_e)
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

/// Same as [`sum_land_yield`] but driven by `Query` system params — for
/// systems that can't take `&World` because of conflicts with `Gizmos` /
/// `ResMut<T>` / mut `Query` (Bevy `B0001`).
pub fn sum_land_yield_q(
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
///
/// Two passes: first collect (entity, gold, levy) per character (immutable borrows),
/// then write the values (mutable borrow). Same trick as [`paying_out::on_month`].
pub fn recompute_yields(world: &mut World) {
    // Pass 1: compute.
    let computed: Vec<(Entity, i64, u64)> = {
        let mut kingdom_holds = world.query::<&KingdomHold>();
        let mut characters = world.query_filtered::<Entity, With<Character>>();
        let mut out: Vec<(Entity, i64, u64)> = Vec::new();
        for char_e in characters.iter(world) {
            let (mut g, mut l) = (0i64, 0u64);
            for kingdom_e in get_character_ruled_kingdoms(world, char_e) {
                let Ok(kingdom_hold) = kingdom_holds.get(world, kingdom_e) else { continue };
                let (dg, dl) = sum_land_yield(kingdom_hold.0, world);
                g += dg;
                l += dl;
            }
            out.push((char_e, g, l));
        }
        out
    };
    // Pass 2: apply.
    let mut characters = world.query_filtered::<(&mut CharacterGoldYield, &mut CharacterLevy), With<Character>>();
    for (char_e, g, l) in computed {
        if let Ok((mut yg, mut lv)) = characters.get_mut(world, char_e) {
            yg.0 = g;
            lv.0 = l;
        }
    }
}

/// Re-sum every kingdom the affected-land's leader rules and write that one leader's yield + levy.
/// The leader's full realm is re-summed, so any change to one of the leader's lands refreshes all.
///
/// All real work happens inside a queued `move |world: &mut World|` closure —
/// Bevy 0.19 forbids observers from taking `&World` alongside `Query<&mut T>` (read-all
/// + write-T conflicts), so we defer the body until after the observer flushes
/// where exclusive world access is fine. Same pattern as
/// [`presenting_event::on_event_resolved`](crate::game::presenting_event::on_event_resolved).
pub fn on_building_updated(
    trigger: On<OnBuildingUpdated>,
    mut commands: Commands,
) {
    let land_e = trigger.event().land;
    commands.queue(move |world: &mut World| {
        if world.get_resource::<Game>().is_none() {
            return;
        }
        let Some(land_held_by) = world.get::<LandHeldBy>(land_e) else { return };
        let kingdom_e = land_held_by.kingdom();
        let Some(leader_e) = get_kingdom_ruler(world, kingdom_e) else { return };

        let (mut g, mut l) = (0i64, 0u64);
        for k in get_character_ruled_kingdoms(world, leader_e) {
            let Some(kingdom_hold) = world.get::<KingdomHold>(k) else { continue };
            let (dg, dl) = sum_land_yield(kingdom_hold.0, world);
            g += dg;
            l += dl;
        }
        if let Ok((mut yg, mut lv)) = world
            .query::<(&mut CharacterGoldYield, &mut CharacterLevy)>()
            .get_mut(world, leader_e)
        {
            yg.0 = g;
            lv.0 = l;
        }
    });
}

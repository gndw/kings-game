//! The daily economy: every kingdom's gold yield and levy recomputed from
//! its holdings, scheduled by the ECS rather than called by hand from
//! `Ctx::tick`. Gold is a realm treasury — each kingdom owns its own gold,
//! yield, and levy.

use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingStatus, KingdomGoldYield, KingdomHold, KingdomLevy, LandHasBuildings,
    LandHeldBy,
};
use crate::helper::kingdom_helper::get_kingdom_ruler;
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

/// Recompute every kingdom's `gold_yield` and `levy` from its land's
/// holdings. Runs in `Startup`. Each kingdom is independent — a leader
/// ruling several kingdoms has access to each kingdom's own gold/levy, not
/// a summed purse.
///
/// Two passes: first compute (immutable borrows), then write (mutable).
/// Same trick as [`paying_out::on_month`].
pub fn recompute_yields(world: &mut World) {
    // Pass 1: compute.
    let computed: Vec<(Entity, i64, u64)> = {
        let mut kingdom_q = world.query::<(Entity, &KingdomHold)>();
        let mut out: Vec<(Entity, i64, u64)> = Vec::new();
        for (k_e, kh) in kingdom_q.iter(world) {
            let (g, l) = sum_land_yield(kh.0, world);
            out.push((k_e, g, l));
        }
        out
    };
    // Pass 2: apply.
    let mut kingdoms = world.query::<(&mut KingdomGoldYield, &mut KingdomLevy)>();
    for (k_e, g, l) in computed {
        if let Ok((mut yg, mut lv)) = kingdoms.get_mut(world, k_e) {
            yg.0 = g;
            lv.0 = l;
        }
    }
}

/// Re-sum the affected kingdom's yield + levy after a building changes.
/// Each kingdom is independent — only the one whose land holds the changed
/// building is recomputed.
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
        // The leader is no longer needed for the calculation — the kingdom
        // is its own economic unit. Kept as a sanity read so a kingdom
        // without a leader still has its yield updated.
        let _ = get_kingdom_ruler(world, kingdom_e);

        let (g, l) = sum_land_yield(land_e, world);
        if let Ok((mut yg, mut lv)) = world
            .query::<(&mut KingdomGoldYield, &mut KingdomLevy)>()
            .get_mut(world, kingdom_e)
        {
            yg.0 = g;
            lv.0 = l;
        }
    });
}

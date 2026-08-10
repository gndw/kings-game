//! The raise-army command: spawn an [`Army`](crate::ecs::army::Army) entity on a
//! land the actor rules.
//!
//! One selection step (pick a ruled land); the action then immediately spawns
//! the army on enter. Reach it through the command palette (**C** then pick
//! *Raise Army*).
//!
//! Initial levy comes from the per-building `BuildingLevy` pool (not from
//! the defs directly): the sum of every ACTIVE building's available levy
//! on the land. The raise then *drains* those pools to `0` and flags the
//! buildings with `BuildingIsRaised = true` so the second raise on the
//! same land is rejected. The monthly
//! [`replenish_levy`](crate::game::replenish_levy::replenish) loop fills
//! the pools back up over time.

use super::core::{
    available_levy, drain_buildings, ruled_lands, Choice, Command, MenuItem, next_id, note,
};
use crate::ecs::army::{Army, ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand, ArmyStatus};
use crate::ecs::{
    CharacterLeads, CharacterOfHouse, HouseName, LandHeldBy, LandName, Registry, StringId,
};
use crate::events::{BuildingUpdateKind, OnArmyRaised, OnBuildingUpdated};
use bevy::ecs::world::World;

/// Raise an army on a land the actor rules.
pub struct RaiseArmy;

impl Command for RaiseArmy {
    fn name(&self) -> &str {
        "Raise Army"
    }

    fn step_count(&self) -> usize {
        1
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select a land",
            _ => "Select a land",
        }
    }

    fn step_items(
        &self,
        _step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        ruled_lands(world, actor)
            .into_iter()
            .map(|(id, name)| MenuItem {
                label: name,
                value: id,
            })
            .collect()
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(land_id) = choices.first().map(|c| c.value.as_str()) else {
            return;
        };
        raise(world, actor, land_id);
    }
}

/// Spawn the army. Validates the actor rules the land, sums the available
/// `BuildingLevy` pools (refusing if none), drains them, creates the army
/// bundle, registers the id, and appends a chronicle line.
fn raise(world: &mut World, actor: &str, land_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot raise on {land_id}: unknown actor"));
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return note(world, format!("cannot raise on {land_id}: no such land"));
    };

    // Rule check: the actor leads the kingdom that holds the land.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let land_k = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    if actor_k.is_none() || actor_k != land_k {
        return note(world, format!("cannot raise on {land_id}: you don't rule that land"));
    }
    let kingdom_e = actor_k.unwrap();

    let land_name = world
        .get::<LandName>(land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| land_id.to_string());

    // Pool gate: refuse when there's no `BuildingLevy` to draw from —
    // either no ACTIVE buildings on the land, or every ACTIVE building's
    // pool is `0` (a previous raise already drained them). Both
    // `!has_levy` and `initial_levy == 0` are checked because
    // `has_levy == true` doesn't guarantee a non-zero sum: an ACTIVE
    // building still flags `has_levy` even when its pool is drained, so
    // a guard on the sum itself is the real test.
    let (initial_levy, has_levy) = available_levy(world, land_e);
    if !has_levy || initial_levy == 0 {
        return note(world, format!(
            "cannot raise on {land_id}: no available levy (wait for the monthly replenishment or dismiss the army in the field)"
        ));
    }

    // Default army name: `<house> Army`, derived from the leader's house.
    // Walk `actor → CharacterOfHouse → HouseName`. A leader with no house
    // falls back to `"Army"` so the field is always populated (and the panel
    // + map label never have to guess).
    let army_name = world
        .get::<CharacterOfHouse>(actor_e)
        .and_then(|coh| world.get::<HouseName>(coh.0))
        .map(|hn| format!("{} Army", hn.0))
        .unwrap_or_else(|| "Army".to_string());

    // Spawn the army bundle. Both `ArmyOnLand` and `ArmyBelongsToKingdom` are
    // Bevy relationships; their hooks fill `LandHasArmies` and
    // `KingdomHasArmies` synchronously, so any later same-frame reader sees
    // authoritative data.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Army,
            ArmyName(army_name.clone()),
            ArmyLevy(initial_levy),
            ArmyOnLand(land_e),
            ArmyBelongsToKingdom(kingdom_e),
            // New armies start idle. The marching tick flips this to
            // `Marching` when activating the first scheduled marching in
            // the queue (starts empty).
            ArmyStatus::Idle,
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    // Drain every ACTIVE building's `BuildingLevy` to 0 and flag it. The
    // spawn above is the source of truth for the new `ArmyLevy`; this
    // keeps the per-building pool in lock-step. The returned list is the
    // buildings that actually transitioned, so we can fire one
    // `OnBuildingUpdated` per building below.
    let drained = drain_buildings(world, land_e);

    note(
        world,
        format!("raised {army_name} on {land_name} ({initial_levy} levy)"),
    );

    // Publish the per-army event so observers see authoritative
    // `LandHasArmies` / `KingdomHasArmies` (the relationship hooks filled
    // them when the bundle spawned above).
    world.trigger(OnArmyRaised { army: eid });
    // Per-building state event: each drained building flipped its
    // `BuildingIsRaised` flag.
    for b_e in drained {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: land_e,
            kind: BuildingUpdateKind::Raised,
        });
    }
}
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
    available_levy, drain_buildings, next_id, note, picker_row, ruled_lands, set_row_selected,
    BaseCommand, NAME_COLOR, STAT_COLOR, STAT_DIM,
};
use crate::app::Game;
use crate::ecs::army::{Army, ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand, ArmyStatus};
use crate::ecs::{
    CharacterLeads, CharacterOfHouse, HouseName, LandHasArmies, LandHeldBy, LandName, Registry,
    StringId,
};
use crate::events::{BuildingUpdateKind, OnArmyRaised, OnBuildingUpdated};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Raise an army on a land the actor rules.
pub struct RaiseArmy;

impl BaseCommand for RaiseArmy {
    fn get_command_id(&self) -> &'static str {
        "command:raise_army"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        // No `"command"` key → render the command row.
        if command_pick.is_none() {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                None,
                "Raise Army",
                NAME_COLOR,
                None,
                None,
                None,
            );
            return (vec![row], false);
        }

        // Mismatch → skip.
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: render one row per land the player rules. Available
        // levy and armies already on the land go in the right cells;
        // lands with no available levy get a `(-no levy)` suffix +
        // dim stat so the player sees it but knows it'll fail.
        let land_pick = choices
            .iter()
            .find(|(k, _)| k == "land_id")
            .map(|(_, v)| v.clone());
        if land_pick.is_none() {
            let actor = world
                .resource::<Game>()
                .ctx
                .player_character_id
                .clone();
            let lands = ruled_lands(world, &actor);
            let mut entities = Vec::new();
            for (land_id, land_name) in lands {
                let land_e = world.resource::<Registry>().get(&land_id);
                let (pool, has_any) = land_e
                    .map(|e| available_levy(world, e))
                    .unwrap_or((0, false));
                let armies_here = land_e
                    .and_then(|e| world.get::<LandHasArmies>(e))
                    .map(|lha| lha.iter().count())
                    .unwrap_or(0);
                let pool_text = if has_any { pool.to_string() } else { String::new() };
                let pool_color = if has_any && pool > 0 { STAT_COLOR } else { STAT_DIM };
                let (name, name_color) = if !has_any || pool == 0 {
                    (format!("{land_name} (no levy)"), super::core::HINT_RED)
                } else {
                    (land_name.clone(), NAME_COLOR)
                };
                let row = picker_row(
                    world,
                    parent,
                    self.get_command_id(),
                    Some(("land_id".to_string(), land_id)),
                    &name,
                    name_color,
                    None,
                    Some((&pool_text, pool_color)),
                    Some((&format!("{armies_here} here"), STAT_DIM)),
                );
                entities.push(row);
            }
            return (entities, false);
        }

        // Execute: both picks present → call the existing function.
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let land_id = land_pick
            .as_deref()
            .expect("step 1 reached without a land_id pick");
        raise(world, &actor, land_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
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

    // Rule check: any of the actor's kingdoms holds the land. Multi-kingdom:
    // the army's kingdom is the specific kingdom that holds the chosen land
    // (so `ArmyBelongsToKingdom` is the holding kingdom, not "the player's
    // kingdom" generically — the player can have several).
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let land_kingdom = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    let kingdom_e = match (actor_kingdoms, land_kingdom) {
        (Some(ks), Some(lk)) if ks.contains(&lk) => lk,
        _ => {
            return note(
                world,
                format!("cannot raise on {land_id}: you don't rule that land"),
            );
        }
    };

    let land_name = world
        .get::<LandName>(land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| land_id.to_string());

    // Pool gate: refuse when there's no `BuildingLevy` to draw from.
    let (initial_levy, has_levy) = available_levy(world, land_e);
    if !has_levy || initial_levy == 0 {
        return note(world, format!(
            "cannot raise on {land_id}: no available levy (wait for the monthly replenishment or dismiss the army in the field)"
        ));
    }

    // Default army name: `<house> Army`.
    let army_name = world
        .get::<CharacterOfHouse>(actor_e)
        
            .and_then(|coh| world.get::<HouseName>(coh.0))
            .map(|hn| format!("{} Army", hn.0))
            .unwrap_or_else(|| "Army".to_string());

    // Spawn the army bundle.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Army,
            ArmyName(army_name.clone()),
            ArmyLevy(initial_levy),
            ArmyOnLand(land_e),
            ArmyBelongsToKingdom(kingdom_e),
            ArmyStatus::Idle,
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    let drained = drain_buildings(world, land_e);

    note(
        world,
        format!("raised {army_name} on {land_name} ({initial_levy} levy)"),
    );

    world.trigger(OnArmyRaised { army: eid });
    for b_e in drained {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: land_e,
            kind: BuildingUpdateKind::Raised,
        });
    }
}

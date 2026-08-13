//! The dismiss-army command: despawn an [`Army`](crate::ecs::army::Army) the actor
//! rules. The inverse of [`super::raise_army`].
//!
//! One selection step listing every army under the actor's kingdom (across all
//! lands, since the kingdom currently owns just one but the listing mirrors
//! the data shape — the army list walks `KingdomHasArmies`). The pick
//! despawns the entity, which Bevy's relationship hooks use to pull it out of
//! the land's `LandHasArmies` and the kingdom's `KingdomHasArmies`; we then
//! deregister the runtime id. The army's levy is distributed back into the
//! kingdom's home land's `BuildingLevy` pools (the ones *raised* drained on
//! the way up) — regardless of which land the army currently sits on. So a
//! dismissed army that marched away still returns its levy home.

use super::core::{
    army_status_text, distribute_levy_back, note, picker_row, set_row_selected, BaseCommand,
    NAME_COLOR, STAT_COLOR,
};
use crate::app::Game;
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyName, ArmyOnLand};
use crate::ecs::kingdom::KingdomHold;
use crate::ecs::{CharacterLeads, KingdomHasArmies, LandName, Registry, StringId};
use crate::events::{BuildingUpdateKind, OnArmyDismiss, OnBuildingUpdated};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Dismiss one of the armies the actor rules.
pub struct DismissArmy;

impl BaseCommand for DismissArmy {
    fn get_command_id(&self) -> &'static str {
        "command:dismiss_army"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        // The current pick (if any) — `(key, value)` where key is
        // `"command"`. Each branch bails out early so the happy path
        // stays at the bottom of the function.
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        // No `"command"` key → first open, render the command row as
        // usual.
        if command_pick.is_none() {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                None,
                "Dismiss Army",
                NAME_COLOR,
                None,
                None,
                None,
            );
            return (vec![row], false);
        }

        // `"command"` key, value mismatch → another command was picked.
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: render one row per army the player rules. Each row
        // shows the army's name, current land on the description line,
        // levy in the first stat cell, and the operational status
        // (idle / marching → dest in N days / sieging at X%) in the
        // second.
        let army_pick = choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone());
        if army_pick.is_none() {
            let actor = world
                .resource::<Game>()
                .ctx
                .player_character_id
                .clone();
            let armies = armies_under(world, &actor);
            let mut entities = Vec::new();
            for (army_id, name, current_land, levy, status) in armies {
                let row = picker_row(
                    world,
                    parent,
                    self.get_command_id(),
                    Some(("army_id".to_string(), army_id)),
                    &name,
                    NAME_COLOR,
                    Some(&format!("at {current_land}")),
                    Some((&levy.to_string(), STAT_COLOR)),
                    status.as_deref().map(|s| (s, STAT_COLOR)),
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
        let army_id = army_pick
            .as_deref()
            .expect("step 1 reached without an army_id pick");
        dismiss(world, &actor, army_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

/// `(army_id, name, current_land, levy, status_text)` for every army
/// the actor rules. Walks the `CharacterLeads` kingdoms and unions
/// their `KingdomHasArmies` lists. `status_text` is `idle` /
/// `→ <land> in <days>d` / `sieging (<progress>%)`, formatted via
/// [`army_status_text`]. None-skipping happens here so a torn-world
/// army doesn't crash the picker.
fn armies_under(
    world: &World,
    actor: &str,
) -> Vec<(String, String, String, u64, Option<String>)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let calendar = world.resource::<Calendar>();
    let date = world.resource::<Date>();
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(*kingdom_e) else {
            continue;
        };
        for army_e in kingdom_has_armies.iter() {
            let Some(string_id) = world.get::<StringId>(army_e) else {
                continue;
            };
            let Some(name) = world.get::<ArmyName>(army_e) else {
                continue;
            };
            let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else {
                continue;
            };
            let current_land = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            let status = army_status_text(world, army_e, calendar, date);
            out.push((string_id.0.clone(), name.0.clone(), current_land, levy, status));
        }
    }
    out
}

/// Despawn the army `army_id` for `actor
/// Validates the actor's kingdom owns
/// the army, then despawns + deregisters. Despawning auto-pulls the army out
/// of both `LandHasArmies` and `KingdomHasArmies` via Bevy's relationship
/// hooks. Any queued marchings under the army are reaped first so the
/// marchings don't outlive their `MarchingArmy` target.
fn dismiss(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot dismiss `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot dismiss `{army_id}`: no such army"));
    };
    // Rule check: the army's `ArmyBelongsToKingdom` is one of the actor's
    // kingdoms (multi-kingdom: any of them counts).
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let kingdom_e = match (actor_kingdoms, army_kingdom) {
        (Some(aks), Some(ak)) if aks.contains(&ak) => ak,
        _ => {
            return note(
                world,
                format!(
                    "cannot dismiss `{army_id}`: that army does not belong to your kingdom"
                ),
            );
        }
    };

    // Two lands to distinguish:
    // - `army_land_e`: the land the army is currently sitting on (for the
    //   chronicle line). The army may have marched away from home.
    // - `kingdom_land_e`: the kingdom's home land — the one whose
    //   `BuildingLevy` pools the army drained on raise, and the one they
    //   fill back into on dismiss. The levy always returns home, not to
    //   whatever land the army happens to be on at dismiss time.
    let army_land_e = world
        .get::<crate::ecs::army::ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0);
    let army_land_name = army_land_e
        .and_then(|e| world.get::<crate::ecs::LandName>(e))
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let kingdom_land_e = world
        .get::<KingdomHold>(kingdom_e)
        .map(|kingdom_hold| kingdom_hold.0);
    let Some(kingdom_land_e) = kingdom_land_e else {
        return note(world, format!("cannot dismiss `{army_id}`: kingdom has no land"));
    };
    let kingdom_land_name = world
        .get::<crate::ecs::LandName>(kingdom_land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());

    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
    let army_levy = world
        .get::<ArmyLevy>(army_e)
        .map(|army_levy| army_levy.0)
        .unwrap_or(0);

    // Distribute the army's levy back into the kingdom's-land buildings
    // BEFORE the despawn.
    let dismissed = distribute_levy_back(world, kingdom_land_e, army_levy);

    // Reap queued marchings first.
    let queued: Vec<bevy::ecs::entity::Entity> = world
        .get::<ArmyHasMarching>(army_e)
        .map(|q| q.iter().collect())
        .unwrap_or_default();
    for m_e in queued {
        world.despawn(m_e);
    }

    // Despawn + deregister.
    world.entity_mut(army_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(army_id);

    note(
        world,
        format!(
            "dismissed {army_name} on {army_land_name} ({army_levy} levy returned to {kingdom_land_name})"
        ),
    );

    world.trigger(OnArmyDismiss { army: army_e });
    for b_e in dismissed {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: kingdom_land_e,
            kind: BuildingUpdateKind::Dismissed,
        });
    }
}

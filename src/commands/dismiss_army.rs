//! The dismiss-army command: despawn an `Army` the actor rules.
//!
//! One step listing every army under the actor's kingdoms. The pick despawns
//! the entity (Bevy's hooks pull it out of `LandHasArmies`/`KingdomHasArmies`),
//! and the army's levy goes back into the kingdom's home land's `BuildingLevy`
/// pools — regardless of which land the army currently sits on.

use super::core::{
    army_status_text, distribute_levy_back, error, picker_row, set_row_selected,
    BaseCommand, NAME_COLOR, STAT_COLOR,
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
        let command_pick = choices.iter().find(|(k, _)| k == "command").map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            let row = picker_row(
                world, parent, self.get_command_id(), None,
                "Dismiss Army", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let army_pick = choices.iter().find(|(k, _)| k == "army_id").map(|(_, v)| v.clone());
        if army_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
            let armies = armies_under(world, &actor);
            let mut entities = Vec::new();
            for (army_id, name, current_land, levy, status) in armies {
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("army_id".to_string(), army_id)),
                    &name, NAME_COLOR,
                    Some(&format!("at {current_land}")),
                    Some((&levy.to_string(), STAT_COLOR)),
                    status.as_deref().map(|s| (s, STAT_COLOR)),
                );
                entities.push(row);
            }
            return (entities, false);
        }

        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let army_id = army_pick.as_deref().expect("step 1 reached without an army_id pick");
        dismiss(world, &actor, army_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

/// `(army_id, name, current_land, levy, status_text)` for every army the actor rules.
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
            let Some(string_id) = world.get::<StringId>(army_e) else { continue };
            let Some(name) = world.get::<ArmyName>(army_e) else { continue };
            let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else { continue };
            let current_land = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);
            let status = army_status_text(world, army_e, calendar, date);
            out.push((string_id.0.clone(), name.0.clone(), current_land, levy, status));
        }
    }
    out
}

/// Despawn the army for the actor. Validates ownership, distributes levy back,
/// reaps queued marchings, despawns + deregisters.
fn dismiss(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot dismiss `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return error(world, format!("cannot dismiss `{army_id}`: no such army"));
    };
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let kingdom_e = match (actor_kingdoms, army_kingdom) {
        (Some(aks), Some(ak)) if aks.contains(&ak) => ak,
        _ => {
            return error(world, format!(
                "cannot dismiss `{army_id}`: that army does not belong to your kingdom"
            ));
        }
    };

    // The kingdom's home land — the buildings whose pools the army drained on raise.
    let kingdom_land_e = world.get::<KingdomHold>(kingdom_e).map(|kh| kh.0);
    let Some(kingdom_land_e) = kingdom_land_e else {
        return error(world, format!("cannot dismiss `{army_id}`: kingdom has no land"));
    };

    let army_levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);

    let dismissed = distribute_levy_back(world, kingdom_land_e, army_levy);

    // Reap queued marchings first.
    let queued: Vec<Entity> = world
        .get::<ArmyHasMarching>(army_e)
        .map(|q| q.iter().collect())
        .unwrap_or_default();
    for m_e in queued {
        world.despawn(m_e);
    }

    world.entity_mut(army_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(army_id);

    world.trigger(OnArmyDismiss { army: army_e });
    for b_e in dismissed {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: kingdom_land_e,
            kind: BuildingUpdateKind::Dismissed,
        });
    }
}

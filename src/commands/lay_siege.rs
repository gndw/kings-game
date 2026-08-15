//! The siege command: lay siege to a land with one of the player's armies.
//!
//! One step: pick an army. The army must be standing on a foreign land. The
//! picked army's current land is the target; the army's `ArmyStatus` flips to
//! `Sieging` and a fresh `Siege` entity is spawned with progress 0 and a first
//! event 10 days out.

use super::core::{error, picker_row, set_row_selected, BaseCommand, NAME_COLOR, STAT_COLOR,
    STAT_DIM};
use crate::app::Game;
use crate::ecs::kingdom::KingdomLedBy;
use crate::ecs::{
    ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand, ArmyStatus, CharacterLeads,
    CharacterName, CharacterOfHouse, HouseName, KingdomHasArmies, LandHeldBy, LandName, Registry,
    Siege, SiegeAttackerArmy, SiegeDefenderLand, SiegeNextEventDate, SiegeProgress, StringId,
};
use crate::observers::OnSiegeLaid;
use crate::ui::command_menu::CommandMenuUiContext;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

pub struct LaySiege;

impl BaseCommand for LaySiege {
    fn get_command_id(&self) -> &'static str {
        "command:lay_siege"
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
                "Lay Siege", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let army_pick = choices.iter().find(|(k, _)| k == "army_id").map(|(_, v)| v.clone());
        match army_pick {
            None => self.spawn_army_picker(world, parent),
            Some(_) => self.execute(world),
        }
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl LaySiege {
    fn spawn_army_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let armies = foreign_army_rows(world, &actor);
        let mut entities = Vec::new();
        for row_data in armies {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("army_id".to_string(), row_data.army_id)),
                &row_data.name, NAME_COLOR,
                row_data.description.as_deref(),
                Some((row_data.levy_text.as_str(), STAT_COLOR)),
                Some((row_data.target_text.as_str(), STAT_DIM)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let army_id = world
            .resource::<CommandMenuUiContext>()
            .choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without an army_id pick");
        begin_siege(world, &actor, &army_id);
        (Vec::new(), true)
    }
}

struct SiegeArmyRow {
    army_id: String,
    name: String,
    description: Option<String>,
    levy_text: String,
    target_text: String,
}

/// `(army_id, name, levy, "<land>, <ruler>")` for every army under the actor's kingdoms on a foreign land.
fn foreign_army_rows(world: &World, actor: &str) -> Vec<SiegeArmyRow> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let actor_kingdoms: std::collections::HashSet<Entity> = character_leads.kingdoms().iter().copied().collect();
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kha) = world.get::<KingdomHasArmies>(*kingdom_e) else { continue };
        for army_e in kha.iter() {
            let (Some(army_id), Some(aol), Some(army_name)) = (
                world.get::<StringId>(army_e).map(|s| s.0.clone()),
                world.get::<ArmyOnLand>(army_e).map(|a| a.0),
                world.get::<ArmyName>(army_e).map(|n| n.0.clone()),
            ) else { continue };
            let is_foreign = world
                .get::<LandHeldBy>(aol)
                .map(|lhb| !actor_kingdoms.contains(&lhb.kingdom()))
                .unwrap_or(false);
            if !is_foreign { continue };
            let levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);
            let land_label = world.get::<LandName>(aol).map(|ln| ln.0.clone()).unwrap_or_else(|| "?".into());
            let target_text = world
                .get::<LandHeldBy>(aol)
                .and_then(|lhb| world.get::<KingdomLedBy>(lhb.kingdom()))
                .map(|kingdom_led_by| {
                    let leader = world
                        .get::<CharacterName>(kingdom_led_by.0)
                        .map(|character_name| character_name.0.clone())
                        .unwrap_or_else(|| "?".into());
                    let house = world
                        .get::<CharacterOfHouse>(kingdom_led_by.0)
                        .and_then(|coh| world.get::<HouseName>(coh.0))
                        .map(|house_name| house_name.0.clone());
                    match house {
                        Some(h) => format!("{land_label}, {leader} {h}"),
                        None => format!("{land_label}, {leader}"),
                    }
                })
                .unwrap_or_else(|| land_label.clone());
            let description = format!("at {land_label}");
            out.push(SiegeArmyRow {
                army_id,
                name: army_name,
                description: Some(description),
                levy_text: levy.to_string(),
                target_text,
            });
        }
    }
    out
}

/// Spawn the siege entity, flip the army to `Sieging`, schedule the first event 10 days out.
fn begin_siege(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot siege with `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return error(world, format!("cannot siege with `{army_id}`: no such army"));
    };

    let (actor_kingdoms, army_land_e, is_foreign) = {
        let actor_kingdoms: std::collections::HashSet<Entity> = world
            .get::<CharacterLeads>(actor_e)
            .map(|cl| cl.kingdoms().iter().copied().collect())
            .unwrap_or_default();
        let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else { return };
        let is_foreign = world
            .get::<LandHeldBy>(army_on_land.0)
            .map(|lhb| !actor_kingdoms.contains(&lhb.kingdom()))
            .unwrap_or(false);
        (actor_kingdoms, army_on_land.0, is_foreign)
    };

    if !world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|abtk| actor_kingdoms.contains(&abtk.0))
        .unwrap_or(false)
    {
        return error(world, format!("cannot siege with `{army_id}`: that army does not belong to your kingdom"));
    }
    if !is_foreign {
        return error(world, format!("cannot siege with `{army_id}`: a siege on your own land is a no-op"));
    }

    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Sieging;
    }
    let today = *world.resource::<crate::resources::date::Date>();
    let next_event = {
        let calendar = world.resource::<crate::resources::calendar::Calendar>();
        today.after_days(10, calendar)
    };
    world.spawn((
        Siege,
        SiegeAttackerArmy(army_e),
        SiegeDefenderLand(army_land_e),
        SiegeProgress(0),
        SiegeNextEventDate(next_event),
    ));

    world.trigger(OnSiegeLaid {
        army: army_e,
        land: army_land_e,
    });
}

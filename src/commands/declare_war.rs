//! The declare-war command: declare war on another kingdom for a casus belli.
//!
//! Two steps: pick a defender kingdom, pick a CB type. Spawns a `War` entity
//! linking the actor's kingdom (attacker) to the defender with the picked CB
//! and an auto-seeded demand list (Conquest → one Take on the defender).

use super::core::{error, next_id, picker_row, set_row_selected, BaseCommand, NAME_COLOR,
    STAT_COLOR};
use crate::ecs::{
    ArmyLevy, CharacterName, CharacterOfHouse, HouseName, Kingdom,
    KingdomHasArmies, KingdomHold, LandName, Registry, StringId, War,
    WarAttackerKingdom, WarBeginDate, WarCasusBelliType, WarDefenderKingdom, WarDemand,
    WarDemandType, WarDemands, WarName,
};
use crate::helper::kingdom_helper::{get_character_ruled_kingdoms, get_kingdom_ruler};
use crate::app::Game;
use crate::observers::OnWarDeclared;
use crate::resources::date::Date;
use crate::ui::command_menu::CommandMenuUiContext;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

pub struct DeclareWar;

impl BaseCommand for DeclareWar {
    fn get_command_id(&self) -> &'static str {
        "command:declare_war"
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
                "Declare War", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let defender_pick = choices.iter().find(|(k, _)| k == "defender_id").map(|(_, v)| v.clone());
        if defender_pick.is_none() {
            return self.spawn_defender_picker(world, parent);
        }

        let cb_pick = choices.iter().find(|(k, _)| k == "cb_id").map(|(_, v)| v.clone());
        if cb_pick.is_none() {
            return self.spawn_cb_picker(world, parent);
        }

        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl DeclareWar {
    fn spawn_defender_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let others = defender_rows(world, &actor);
        let mut entities = Vec::new();
        for row_data in others {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("defender_id".to_string(), row_data.kingdom_id)),
                &row_data.name, NAME_COLOR,
                row_data.description.as_deref(),
                Some((row_data.ruler.as_str(), STAT_COLOR)),
                Some((row_data.strength.as_str(), STAT_COLOR)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_cb_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = picker_row(
            world, parent, self.get_command_id(),
            Some(("cb_id".to_string(), "conquest".to_string())),
            "Conquest", NAME_COLOR,
            Some("seize their land"),
            None, None,
        );
        (vec![row], false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let picks: Vec<(String, String)> = world.resource::<CommandMenuUiContext>().choices.clone();
        let defender_id = picks.iter().find(|(k, _)| k == "defender_id").map(|(_, v)| v.clone())
            .expect("execute reached without a defender_id pick");
        let cb_id = picks.iter().find(|(k, _)| k == "cb_id").map(|(_, v)| v.clone())
            .expect("execute reached without a cb_id pick");
        declare(world, &actor, &defender_id, &cb_id);
        (Vec::new(), true)
    }
}

struct DefenderRowData {
    kingdom_id: String,
    name: String,
    description: Option<String>,
    ruler: String,
    strength: String,
}

/// One row per kingdom the actor doesn't already lead.
fn defender_rows(world: &World, actor: &str) -> Vec<DefenderRowData> {
    let own_kingdoms: std::collections::HashSet<bevy::ecs::entity::Entity> = world
        .resource::<Registry>()
        .get(actor)
        .map(|actor_e| get_character_ruled_kingdoms(world, actor_e))
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Kingdom>().is_none() {
            continue;
        }
        let kingdom_e = entity_ref.id();
        if own_kingdoms.contains(&kingdom_e) {
            continue;
        }
        let Some(string_id) = entity_ref.get::<StringId>() else { continue };
        let land_label = entity_ref
            .get::<KingdomHold>()
            .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
            .map(|land_name| land_name.0.clone())
            .unwrap_or_else(|| string_id.0.clone());

        let ruler_e = get_kingdom_ruler(world, kingdom_e);
        let ruler = ruler_e
            .and_then(|e| world.get::<CharacterName>(e))
            .map(|character_name| character_name.0.clone())
            .unwrap_or_default();
        let ruler_with_house = if ruler.is_empty() {
            String::new()
        } else {
            let house = ruler_e
                .and_then(|e| world.get::<CharacterOfHouse>(e))
                .and_then(|character_of_house| world.get::<HouseName>(character_of_house.0))
                .map(|house_name| house_name.0.clone());
            match house {
                Some(h) => format!("{ruler}, {h}"),
                None => ruler,
            }
        };

        let (army_count, total_levy) = entity_ref
            .get::<KingdomHasArmies>()
            .map(|kingdom_has_armies| {
                let count = kingdom_has_armies.iter().count();
                let levy: u64 = kingdom_has_armies
                    .iter()
                    .filter_map(|army_e| world.get::<ArmyLevy>(army_e).map(|army_levy| army_levy.0))
                    .sum();
                (count, levy)
            })
            .unwrap_or((0, 0));
        let strength = if army_count > 0 {
            format!("{army_count} here, {total_levy} levy")
        } else {
            String::new()
        };

        result.push(DefenderRowData {
            kingdom_id: string_id.0.clone(),
            name: land_label,
            description: if ruler_with_house.is_empty() { None } else { Some(ruler_with_house.clone()) },
            ruler: ruler_with_house,
            strength,
        });
    }
    result
}

/// Resolve the picked CB id to its `WarCasusBelliType`. Unknown ids are rejected.
fn resolve_cb(cb_id: &str) -> Option<WarCasusBelliType> {
    match cb_id {
        "conquest" => Some(WarCasusBelliType::Conquest),
        _ => None,
    }
}

/// Seed the war's initial demands from the picked CB type + the defender kingdom.
fn demands_for(cb_type: WarCasusBelliType, defender_kingdom_e: bevy::ecs::entity::Entity) -> Vec<WarDemand> {
    match cb_type {
        WarCasusBelliType::Conquest => vec![WarDemand {
            demand_type: WarDemandType::Take,
            target: defender_kingdom_e,
        }],
    }
}

/// Validate, then spawn a `War` entity linking the actor's kingdom to the defender.
fn declare(world: &mut World, actor: &str, defender_id: &str, cb_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, "cannot declare war: unknown actor".into());
    };
    let Some(attacker_kingdom_e) = get_character_ruled_kingdoms(world, actor_e).first().copied()
    else {
        return error(world, "cannot declare war: you rule no kingdom".into());
    };
    let Some(defender_kingdom_e) = world.resource::<Registry>().get(defender_id) else {
        return error(world, format!("cannot declare war: no such kingdom `{defender_id}`"));
    };
    if defender_kingdom_e == attacker_kingdom_e {
        return error(world, "cannot declare war on yourself".into());
    }
    let Some(cb_type) = resolve_cb(cb_id) else {
        return error(world, format!("unknown casus belli `{cb_id}`"));
    };

    let demands = demands_for(cb_type, defender_kingdom_e);

    let war_entity_id = next_id(world);
    let begin_date = *world.resource::<Date>();
    let war_name = format_name(world, cb_type, defender_kingdom_e);
    let war_e = world
        .spawn((
            StringId(war_entity_id.clone()),
            War,
            WarAttackerKingdom(attacker_kingdom_e),
            WarDefenderKingdom(defender_kingdom_e),
            cb_type,
            WarDemands(demands),
            WarName(war_name),
            WarBeginDate(begin_date),
        ))
        .id();
    world.resource_mut::<Registry>().insert(war_entity_id, war_e);

    world.trigger(OnWarDeclared {
        attacker: attacker_kingdom_e,
        defender: defender_kingdom_e,
        casus_belli: cb_type,
    });
}

/// Display label for a kingdom: the name of its held land, falling back to the kingdom's id.
fn kingdom_label(world: &World, kingdom_e: bevy::ecs::entity::Entity) -> String {
    world
        .get::<KingdomHold>(kingdom_e)
        .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| {
            world
                .get::<StringId>(kingdom_e)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| "?".into())
        })
}

/// Format a war's display name from the CB type + the defender kingdom's held land.
fn format_name(
    world: &World,
    cb_type: WarCasusBelliType,
    defender_kingdom_e: bevy::ecs::entity::Entity,
) -> String {
    let land = kingdom_label(world, defender_kingdom_e);
    match cb_type {
        WarCasusBelliType::Conquest => format!("Conquest over Kingdom of {land}"),
    }
}

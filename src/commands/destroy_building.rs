//! The destroy-building command: tear down a building instance on a land the actor rules.

use super::core::{
    error, land_yield, picker_row, ruled_lands, set_row_selected, BaseCommand, NAME_COLOR,
    STAT_COLOR, STAT_DIM,
};
use crate::ecs::{
    BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand,
    BuildingStatus, LandHasBuildings, LandHeldBy, Registry, StringId,
};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
use crate::observers::{BuildingUpdateKind, OnBuildingUpdated};
use crate::resources::buildings::BuildingDefs;
use crate::app::Game;
use crate::commands::construct_building::{building_effect_summary, format_gold, format_levy};
use crate::ui::command_menu::CommandMenuUiContext;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

pub struct DestroyBuilding;

impl BaseCommand for DestroyBuilding {
    fn get_command_id(&self) -> &'static str {
        "command:destroy_building"
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
                "Destroy Building", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let land_pick = choices.iter().find(|(k, _)| k == "land_id").map(|(_, v)| v.clone());
        if land_pick.is_none() {
            return self.spawn_land_picker(world, parent);
        }

        let building_pick = choices.iter().find(|(k, _)| k == "building_id").map(|(_, v)| v.clone());
        if building_pick.is_none() {
            return self.spawn_building_picker(world, parent, &land_pick.unwrap());
        }

        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl DestroyBuilding {
    fn spawn_land_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let lands = ruled_lands(world, &actor);
        let mut entities = Vec::new();
        for (land_id, land_name) in lands {
            let (g, l) = world
                .resource::<Registry>()
                .get(&land_id)
                .map(|e| land_yield(world, e))
                .unwrap_or((0, 0));
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("land_id".to_string(), land_id)),
                &land_name, NAME_COLOR, None,
                Some((&format_gold(g), STAT_COLOR)),
                Some((&format_levy(l), STAT_COLOR)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_building_picker(
        &self,
        world: &mut World,
        parent: Entity,
        land_id: &str,
    ) -> (Vec<Entity>, bool) {
        let rows = buildings_on_land(world, land_id);
        let mut entities = Vec::new();
        for row_data in rows {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("building_id".to_string(), row_data.instance_id)),
                &row_data.name, row_data.name_color,
                row_data.description.as_deref(),
                Some((row_data.state_text.as_str(), STAT_COLOR)),
                Some((row_data.pool_text.as_str(), STAT_DIM)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let picks: Vec<(String, String)> = world.resource::<CommandMenuUiContext>().choices.clone();
        let land_id = picks.iter().find(|(k, _)| k == "land_id").map(|(_, v)| v.clone())
            .expect("execute reached without a land_id pick");
        let building_id = picks.iter().find(|(k, _)| k == "building_id").map(|(_, v)| v.clone())
            .expect("execute reached without a building_id pick");
        destroy(world, &actor, &land_id, &building_id);
        (Vec::new(), true)
    }
}

struct BuildingRowData {
    instance_id: String,
    name: String,
    name_color: Color,
    description: Option<String>,
    state_text: String,
    pool_text: String,
}

/// Read every building instance standing on `land_id` and assemble its picker row data.
fn buildings_on_land(world: &World, land_id: &str) -> Vec<BuildingRowData> {
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return Vec::new();
    };
    let Some(land_has_buildings) = world.get::<LandHasBuildings>(land_e) else {
        return Vec::new();
    };
    let defs = world.resource::<BuildingDefs>();
    let calendar = world.resource::<Calendar>();
    let today = *world.resource::<Date>();
    let today_ord = today.ordinal(&calendar);

    land_has_buildings
        .iter()
        .filter_map(|b_e| {
            let string_id = world.get::<StringId>(b_e)?.0.clone();
            let building_of = world.get::<BuildingOf>(b_e)?;
            let def = defs.get(&building_of.0);
            let status = world.get::<BuildingStatus>(b_e).copied().unwrap_or(BuildingStatus::Active);
            let pool = world.get::<BuildingLevy>(b_e).copied().unwrap_or(BuildingLevy(0)).0;
            let is_raised = world.get::<BuildingIsRaised>(b_e).copied().unwrap_or(BuildingIsRaised(false)).0;

            let def_name = def.map(|d| d.name.clone()).unwrap_or_else(|| building_of.0.clone());
            let (name, hint) = match status {
                BuildingStatus::Building => (format!("{def_name} (building)"), true),
                BuildingStatus::Inactive => (format!("{def_name} (inactive)"), true),
                BuildingStatus::Active if is_raised => (format!("{def_name} (in field)"), true),
                BuildingStatus::Active => (def_name.clone(), false),
            };

            let state_text = match status {
                BuildingStatus::Building => world
                    .get::<BuildingConstructionDate>(b_e)
                    .map(|BuildingConstructionDate(finish)| {
                        let remaining = (finish.ordinal(&calendar) - today_ord).max(0) as u32;
                        calendar.format_duration(remaining)
                    })
                    .unwrap_or_else(|| "?".into()),
                BuildingStatus::Inactive => "inactive".into(),
                BuildingStatus::Active => "active".into(),
            };

            let pool_text = if is_raised {
                "raised".into()
            } else if let Some(d) = def {
                if d.levy > 0 { format!("{}/{}", pool, d.levy) } else { String::new() }
            } else {
                String::new()
            };

            let description = def.map(building_effect_summary);

            Some(BuildingRowData {
                instance_id: string_id,
                name,
                name_color: if hint { super::core::HINT_RED } else { NAME_COLOR },
                description,
                state_text,
                pool_text,
            })
        })
        .collect()
}

/// Destroy the building on `land_id` for `actor`. Validates ownership, despawns + deregisters.
fn destroy(world: &mut World, actor: &str, land_id: &str, building_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot destroy on {land_id}: unknown actor"));
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return error(world, format!("cannot destroy on {land_id}: no such land"));
    };

    let actor_kingdoms = get_character_ruled_kingdoms(world, actor_e);
    let land_kingdom = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    match (actor_kingdoms, land_kingdom) {
        (ks, Some(lk)) if ks.contains(&lk) => {}
        _ => {
            return error(world, format!("cannot destroy on {land_id}: you don't rule that land"));
        }
    }

    let Some(b_e) = world.resource::<Registry>().get(building_id) else {
        return error(world, format!("cannot destroy on {land_id}: no such building"));
    };
    if world.get::<BuildingOnLand>(b_e).map(|bol| bol.0) != Some(land_e) {
        return error(world, format!("cannot destroy on {land_id}: building not on that land"));
    }

    world.entity_mut(b_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(building_id);

    world.trigger(OnBuildingUpdated {
        building: b_e,
        land: land_e,
        kind: BuildingUpdateKind::Destroyed,
    });
}

//! The destroy-building command: tear down a building instance standing on a
//! land the actor rules. The inverse of [`super::construct_building`].
//!
//! Despawning the entity auto-pulls it from the land's
//! [`LandHasBuildings`](crate::ecs::LandHasBuildings) (the relationship hook);
//! we then fire `OnBuildingUpdated` so
//! [`on_building_updated`](crate::game::yields::on_building_updated)
//! re-sums the realm against the post-hook `LandHasBuildings`.
//!
//! [`recompute_yields`]: crate::game::yields::recompute_yields

use super::core::{
    land_yield, note, picker_row, ruled_lands, set_row_selected, BaseCommand, NAME_COLOR,
    STAT_COLOR, STAT_DIM,
};
use crate::ecs::{
    BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand,
    BuildingStatus, CharacterLeads, LandHasBuildings, LandHeldBy, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use crate::app::Game;
use crate::commands::construct_building::{building_effect_summary, format_gold, format_levy};
use crate::ui::command_menu::CommandMenuUiContext;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

/// Tear down a building on a land the actor rules.
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
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                None,
                "Destroy Building",
                NAME_COLOR,
                None,
                None,
                None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: pick a land.
        let land_pick = choices
            .iter()
            .find(|(k, _)| k == "land_id")
            .map(|(_, v)| v.clone());
        if land_pick.is_none() {
            return self.spawn_land_picker(world, parent);
        }

        // Step 2: pick a building on that land.
        let building_pick = choices
            .iter()
            .find(|(k, _)| k == "building_id")
            .map(|(_, v)| v.clone());
        if building_pick.is_none() {
            return self.spawn_building_picker(world, parent, &land_pick.unwrap());
        }

        // Execute.
        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl DestroyBuilding {
    fn spawn_land_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let lands = ruled_lands(world, &actor);
        let mut entities = Vec::new();
        for (land_id, land_name) in lands {
            let (g, l) = world
                .resource::<Registry>()
                .get(&land_id)
                .map(|e| land_yield(world, e))
                .unwrap_or((0, 0));
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("land_id".to_string(), land_id)),
                &land_name,
                NAME_COLOR,
                None,
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
        // Snapshot every building on the land (entity + display data)
        // up-front so the immutable borrows drop before we spawn rows.
        let rows = buildings_on_land(world, land_id);
        let mut entities = Vec::new();
        for row_data in rows {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("building_id".to_string(), row_data.instance_id)),
                &row_data.name,
                row_data.name_color,
                row_data.description.as_deref(),
                Some((row_data.state_text.as_str(), STAT_COLOR)),
                Some((row_data.pool_text.as_str(), STAT_DIM)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let picks: Vec<(String, String)> =
            world.resource::<CommandMenuUiContext>().choices.clone();
        let land_id = picks
            .iter()
            .find(|(k, _)| k == "land_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a land_id pick");
        let building_id = picks
            .iter()
            .find(|(k, _)| k == "building_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a building_id pick");
        destroy(world, &actor, &land_id, &building_id);
        (Vec::new(), true)
    }
}

/// One building-on-land row's display data, precomputed in
/// [`buildings_on_land`] so the picker can spawn rows without holding
/// borrows on the world. `name_color` is `HINT_RED` for raised /
/// non-active buildings so the player sees which are unsafe to tear
/// down (the `validate` path actually allows it — the colour is a hint,
/// not a disabled state).
struct BuildingRowData {
    instance_id: String,
    name: String,
    name_color: Color,
    description: Option<String>,
    state_text: String,
    pool_text: String,
}

/// Read every building instance standing on `land_id` and assemble its
/// picker row data. Status drives the right-column text: BUILDING
/// shows days remaining, INACTIVE shows `inactive`, ACTIVE shows the
/// pool's current/max (or `raised` when the pool is in an army).
/// Raised / non-active rows get a red name tint.
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
            let status = world
                .get::<BuildingStatus>(b_e)
                .copied()
                .unwrap_or(BuildingStatus::Active);
            let pool = world
                .get::<BuildingLevy>(b_e)
                .copied()
                .unwrap_or(BuildingLevy(0))
                .0;
            let is_raised = world
                .get::<BuildingIsRaised>(b_e)
                .copied()
                .unwrap_or(BuildingIsRaised(false))
                .0;

            let def_name = def.map(|d| d.name.clone()).unwrap_or_else(|| building_of.0.clone());
            // Suffix on the name communicates the lifecycle state —
            // plain name for the active, ready-to-destroy case.
            let (name, hint) = match status {
                BuildingStatus::Building => (format!("{def_name} (building)"), true),
                BuildingStatus::Inactive => (format!("{def_name} (inactive)"), true),
                BuildingStatus::Active if is_raised => (format!("{def_name} (in field)"), true),
                BuildingStatus::Active => (def_name.clone(), false),
            };

            // Right cell 1: lifecycle readout. BUILDING → days left;
            // INACTIVE → "inactive"; ACTIVE → "active". Calendar's
            // ordinal math gives days-from-finish-to-today.
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

            // Right cell 2: pool. Raised short-circuits to a label so
            // the player sees "this levy is in an army" without
            // arithmetic; otherwise show the `current/max` fraction
            // (or just `max` when the pool is full).
            let pool_text = if is_raised {
                "raised".into()
            } else if let Some(d) = def {
                if pool >= d.levy {
                    format!("{}/{}", pool, d.levy)
                } else if d.levy > 0 {
                    format!("{}/{}", pool, d.levy)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Description line: same effect summary the construct
            // picker uses, so the player can compare what they'd lose.
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

/// Destroy the building `building_id` on `land_id` for `actor`. Validates the
/// actor rules the land and the building is on it, then despawns + deregisters
/// + logs.
fn destroy(world: &mut World, actor: &str, land_id: &str, building_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot destroy on {land_id}: unknown actor"));
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return note(world, format!("cannot destroy on {land_id}: no such land"));
    };

    // Rule check: any of the actor's kingdoms holds the land.
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let land_kingdom = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    match (actor_kingdoms, land_kingdom) {
        (Some(ks), Some(lk)) if ks.contains(&lk) => {}
        _ => {
            return note(
                world,
                format!("cannot destroy on {land_id}: you don't rule that land"),
            );
        }
    }

    let Some(b_e) = world.resource::<Registry>().get(building_id) else {
        return note(world, format!("cannot destroy on {land_id}: no such building"));
    };
    if world
        .get::<BuildingOnLand>(b_e)
        .map(|building_on_land| building_on_land.0)
        != Some(land_e)
    {
        return note(world, format!("cannot destroy on {land_id}: building not on that land"));
    }

    // Def name for the log, looked up before the despawn drops the component.
    let def_name = world
        .get::<BuildingOf>(b_e)
        .and_then(|building_of| {
            world
                .resource::<BuildingDefs>()
                .get(&building_of.0)
                .map(|d| d.name.clone())
        })
        .unwrap_or_else(|| building_id.to_string());

    // Despawn + deregister. `BuildingOnLand`'s hook pulls the building out of
    // the land's `LandHasBuildings` synchronously.
    world.entity_mut(b_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(building_id);

    note(world, format!("destroyed {} on {}", def_name, land_id));
}

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

use super::core::{note, BaseCommand};
use crate::ecs::{
    BuildingOf, BuildingOnLand, CharacterLeads, LandHasBuildings, LandHeldBy, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use crate::app::Game;
use crate::commands::core::ruled_lands;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue, CommandMenuUiContext};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

/// Tear down a building on a land the actor rules.
pub struct DestroyBuilding;

// --- palette UI -------------------------------------------------------------
// Same shape as `construct_building`: a single padded card whose title
// text is the command's display name. The shared `update` swaps the
// background between `ROW_PANEL` and `ROW_PANEL_SELECTED`.

/// Per-row background in the palette. One shade lighter than the panel.
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);

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
            return self.spawn_command_row(world, parent);
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
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

impl DestroyBuilding {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = self.spawn_row(world, parent, "Destroy Building", None);
        (vec![row], false)
    }

    fn spawn_land_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let lands = ruled_lands(world, &actor);
        let mut entities = Vec::new();
        for (land_id, name) in lands {
            let row = self.spawn_row(
                world,
                parent,
                &name,
                Some(("land_id".to_string(), land_id)),
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
        let buildings = buildings_on_land(world, land_id);
        let mut entities = Vec::new();
        for (building_id, label) in buildings {
            let row = self.spawn_row(
                world,
                parent,
                &label,
                Some(("building_id".to_string(), building_id)),
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

    fn spawn_row(
        &self,
        world: &mut World,
        parent: Entity,
        title: &str,
        key_value: Option<(String, String)>,
    ) -> Entity {
        let mut entity = world.spawn((
            Node {
                width: percent(100),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(ROW_PANEL),
            BorderColor::all(ROW_BORDER),
            ChildOf(parent),
            CommandHasId(self.get_command_id().to_string()),
        ));
        if let Some((k, v)) = key_value {
            entity.insert((CommandHasKey(k), CommandHasValue(v)));
        }
        let row = entity.id();
        world.entity_mut(row).with_children(|c| {
            c.spawn((
                Text::new(title),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
        row
    }
}

/// `(building_instance_id, "Name  (destroy)")` for every building standing on
/// `land_id`, in the land's [`LandHasBuildings`] order. The instance id is the
/// value the command hands back to [`destroy`]. Walks the relationship target
/// with `world::get` so it stays a `&World` read (`world::query` needs `&mut
/// World`).
fn buildings_on_land(world: &World, land_id: &str) -> Vec<(String, String)> {
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return Vec::new();
    };
    let Some(land_has_buildings) = world.get::<LandHasBuildings>(land_e) else {
        return Vec::new();
    };
    let defs = world.resource::<BuildingDefs>();
    land_has_buildings
        .iter()
        .filter_map(|b_e| {
            let string_id = world.get::<StringId>(b_e)?;
            let building_of = world.get::<BuildingOf>(b_e)?;
            let label = match defs.get(&building_of.0) {
                Some(d) => format!("{}  (destroy)", d.name),
                None => format!("{}  (destroy)", building_of.0),
            };
            Some((string_id.0.clone(), label))
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

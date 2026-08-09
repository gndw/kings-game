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

use super::core::{Choice, Command, MenuItem, note, ruled_lands};
use crate::ecs::{
    BuildingOf, BuildingOnLand, CharacterLeads, LandHasBuildings, LandHeldBy, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;

/// Tear down a building on a land the actor rules.
pub struct DestroyBuilding;

impl Command for DestroyBuilding {
    fn name(&self) -> &str {
        "Destroy Building"
    }

    fn step_count(&self) -> usize {
        2
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select a land",
            _ => "Select a building to destroy",
        }
    }

    fn step_items(
        &self,
        step: usize,
        choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        match step {
            // Step 0: the lands the actor rules.
            0 => ruled_lands(world, actor)
                .into_iter()
                .map(|(id, name)| MenuItem { label: name, value: id })
                .collect(),
            // Step 1: the buildings standing on the land chosen at step 0.
            _ => {
                let land_id = choices.first().map(|c| c.value.as_str()).unwrap_or("");
                buildings_on_land(world, land_id)
                    .into_iter()
                    .map(|(id, label)| MenuItem { label, value: id })
                    .collect()
            }
        }
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(land_id) = choices.get(0).map(|c| c.value.as_str()) else {
            return;
        };
        let Some(building_id) = choices.get(1).map(|c| c.value.as_str()) else {
            return;
        };
        destroy(world, actor, land_id, building_id);
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

    // Rule check: the actor leads the kingdom that holds the land.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let land_k = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    if actor_k.is_none() || actor_k != land_k {
        return note(world, format!("cannot destroy on {land_id}: you don't rule that land"));
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
    // the land's `LandHasBuildings` synchronously, so the yield observer can
    // re-sum authoritative data on the next line.
    world.entity_mut(b_e).despawn();
    world.trigger(crate::events::OnBuildingUpdated {
        building: b_e,
        land: land_e,
        kind: crate::events::BuildingUpdateKind::Destroyed,
    });
    world.resource_mut::<Registry>().by_id.remove(building_id);

    note(world, format!("destroyed {} on {}", def_name, land_id));
}

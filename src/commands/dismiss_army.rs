//! The dismiss-army command: despawn an [`Army`](crate::ecs::army::Army) the actor
//! rules. The inverse of [`super::raise_army`].
//!
//! One selection step listing every army under the actor's kingdom (across all
//! lands, since the kingdom currently owns just one but the listing mirrors
//! the data shape — the army list walks `KingdomHasArmies`). The pick
//! despawns the entity, which Bevy's relationship hooks use to pull it out of
//! the land's `LandHasArmies` and the kingdom's `KingdomHasArmies`; we then
//! deregister the runtime id.

use super::core::{distribute_levy_back, Choice, Command, MenuItem, note};
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyName};
use crate::ecs::{CharacterLeads, KingdomHasArmies, Registry, StringId};
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;

/// Dismiss one of the armies the actor rules.
pub struct DismissArmy;

impl Command for DismissArmy {
    fn name(&self) -> &str {
        "Dismiss Army"
    }

    fn step_count(&self) -> usize {
        1
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select an army",
            _ => "Select an army",
        }
    }

    fn step_items(
        &self,
        _step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        armies_under(world, actor)
            .into_iter()
            .map(|(id, label)| MenuItem { label, value: id })
            .collect()
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(army_id) = choices.first().map(|c| c.value.as_str()) else {
            return;
        };
        dismiss(world, actor, army_id);
    }
}

/// `(army_instance_id, "<land>:<levy>")` for every army the actor's kingdom
/// rules, in `KingdomHasArmies` order. Walks the relationship target via
/// `world::get` so it stays a `&World` read.
fn armies_under(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(kingdom_e) = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom())
    else {
        return Vec::new();
    };
    let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(kingdom_e) else {
        return Vec::new();
    };
    kingdom_has_armies
        .iter()
        .filter_map(|army_e| {
            let string_id = world.get::<StringId>(army_e)?;
            // For the label we need the land name (army → land → name) and the
            // levy count. Army→land is via `ArmyOnLand`; the levy is
            // `ArmyLevy`. Both reads are `world::get` so they stay `&World`.
            let army_on_land = world.get::<crate::ecs::army::ArmyOnLand>(army_e)?;
            let land_name = world
                .get::<crate::ecs::LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            Some((
                string_id.0.clone(),
                format!("{land_name}: {levy}"),
            ))
        })
        .collect()
}

/// Despawn the army `army_id` for `actor`. Validates the actor's kingdom owns
/// the army, then despawns + deregisters. Despawning auto-pulls the army out
/// of both `LandHasArmies` and `KingdomHasArmies` via Bevy's relationship
/// hooks.
fn dismiss(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot dismiss `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot dismiss `{army_id}`: no such army"));
    };
    // Rule check: the actor leads a kingdom, and that kingdom is the army's
    // `ArmyBelongsToKingdom` target.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let army_k = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    if actor_k.is_none() || actor_k != army_k {
        return note(world, format!(
            "cannot dismiss `{army_id}`: that army does not belong to your kingdom"
        ));
    }

    // Label for the log: land name + army name + levy before we drop the
    // components. Read in this order so the relationships are still valid.
    let land_name = world
        .get::<crate::ecs::army::ArmyOnLand>(army_e)
        .and_then(|army_on_land| {
            world
                .get::<crate::ecs::LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
        })
        .unwrap_or_else(|| "?".into());
    let land_e = world
        .get::<crate::ecs::army::ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0);
    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
    let army_levy = world
        .get::<ArmyLevy>(army_e)
        .map(|army_levy| army_levy.0)
        .unwrap_or(0);

    // Distribute the army's levy back into the land's buildings BEFORE the
    // despawn — `distribute_levy_back` walks `LandHasBuildings` on the
    // army's land; once the army is gone, the land's auto-maintained
    // collection is still correct (it lists *buildings*, not armies).
    if let Some(land_e) = land_e {
        distribute_levy_back(world, land_e, army_levy);
    }

    // Despawn any queued marchings BEFORE the army goes — otherwise the
    // marchings would be left holding a `MarchingArmy` pointing at a
    // despawned entity. Bevy's relationship hooks drop the `MarchingArmy`
    // off the marching on target despawn, but the marching itself would
    // still be in the world with no army, no source land, and no status
    // to walk. Snapshot the marching entities, then drop the borrow
    // before despawning.
    let queued: Vec<bevy::ecs::entity::Entity> = world
        .get::<ArmyHasMarching>(army_e)
        .map(|q| q.iter().collect())
        .unwrap_or_default();
    for m_e in queued {
        world.despawn(m_e);
    }

    // Despawn + deregister. The relationship hooks remove the army from the
    // land's `LandHasArmies` and the kingdom's `KingdomHasArmies` in the same
    // operation.
    world.entity_mut(army_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(army_id);

    note(
        world,
        format!("dismissed {army_name} on {land_name} ({army_levy} levy returned)"),
    );
}
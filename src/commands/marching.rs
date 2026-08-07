//! The marching-army command: queue a marching order to move one of the
//! actor's armies to another land. The marching is a separate entity
//! (`Marching` + relationships + dates) that the daily marching tick
//! activates when the army is sitting on the marching's source land.
//!
//! Two selection steps: step 0 picks an army (any army under the actor's
//! kingdom), step 1 picks the target land. The actor must rule the army's
//! kingdom (via `ArmyBelongsToKingdom`) and the target land must be a
//! different land from the army's current land.
//!
//! From the actions panel the **G** hotkey opens the palette directly into
//! step 1 with the first army on the selected land pre-picked (mirroring
//! how **B**/**D** open the construct/destroy palette). The "G" row only
//! shows when the player rules the selected land AND at least one player's
//! army sits on it.
//!
//! [`MarchingArmy::execute`] just spawns the marching entity and walks
//! away — the daily tick does the actual lifting (activating the queued
//! marching, moving the army, advancing to the next one in the queue).
//! See [`crate::game::marching::tick`].

use super::core::{Choice, Command, MenuItem, next_id, note};
use crate::ecs::army::{
    ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand,
};
use crate::ecs::kingdom::KingdomHasArmies;
use crate::ecs::marching::{
    Marching, MarchingArmy, MarchingArrivedDate, MarchingBeginDate, MarchingFromLand,
    MarchingStatus, MarchingToLand,
};
use crate::ecs::{CharacterLeads, LandName, Registry, StringId};
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;

/// Queue a marching order for one of the actor's armies. Registered as
/// "Marching Army" in the command palette (the player-facing name); the
/// struct is named `MarchingOrder` to avoid colliding with the
/// [`MarchingArmy`](crate::ecs::marching::MarchingArmy) component in
/// `ecs::marching`.
pub struct MarchingOrder;

impl Command for MarchingOrder {
    fn name(&self) -> &str {
        "Marching Army"
    }

    fn step_count(&self) -> usize {
        2
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select an army",
            _ => "Select a target land",
        }
    }

    fn step_items(
        &self,
        step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        match step {
            0 => armies_under(world, actor)
                .into_iter()
                .map(|(id, label)| MenuItem { label, value: id })
                .collect(),
            _ => all_lands(world)
                .into_iter()
                .map(|(id, name)| MenuItem { label: name, value: id })
                .collect(),
        }
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(army_id) = choices.get(0).map(|c| c.value.as_str()) else {
            return;
        };
        let Some(target_id) = choices.get(1).map(|c| c.value.as_str()) else {
            return;
        };
        march(world, actor, army_id, target_id);
    }
}

/// `(army_id, "<land>: <levy>")` for every army under the actor's kingdom,
/// in `KingdomHasArmies` order. Mirrors `dismiss_army::armies_under` — the
/// step-0 list for the marching command is the same shape as the step-0 list
/// for the dismiss command, since both commands pick from the actor's army
/// pool.
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
            let army_id = world.get::<StringId>(army_e)?.0.clone();
            let army_on_land = world.get::<ArmyOnLand>(army_e)?;
            let land_name = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            Some((army_id, format!("{land_name}: {levy}")))
        })
        .collect()
}

/// `(land_id, land_name)` for every land in the world. The player picks
/// the target; `execute` rejects the same land as the army's current land.
/// Filters by the `Land` marker so we only see land entities (every land
/// carries `LandName`; the marker is the canonical land discriminant).
/// Uses `world.iter_entities()` (`&World` safe) rather than `world.query`
/// (which needs `&mut World`).
fn all_lands(world: &World) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<crate::ecs::Land>().is_none() {
            continue;
        }
        let (Some(sid), Some(name)) = (
            entity_ref.get::<StringId>(),
            entity_ref.get::<LandName>(),
        ) else {
            continue;
        };
        result.push((sid.0.clone(), name.0.clone()));
    }
    result
}

/// Validate (actor rules the army; target is a different land), then spawn
/// the marching entity with `MarchingStatus::Scheduled` and empty dates.
/// The daily tick will activate it when the army is on the matching source
/// land.
fn march(world: &mut World, actor: &str, army_id: &str, target_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot march `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot march `{army_id}`: no such army"));
    };
    let Some(target_e) = world.resource::<Registry>().get(target_id) else {
        return note(world, format!("cannot march to `{target_id}`: no such land"));
    };

    // Rule check: the actor leads the army's kingdom.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let army_k = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    if actor_k.is_none() || actor_k != army_k {
        return note(world, format!(
            "cannot march `{army_id}`: that army does not belong to your kingdom"
        ));
    }

    // The army's current land is the source. Capture it before we mutate
    // anything so the chronicle line can name it.
    let from_e = world
        .get::<ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0);
    let Some(from_e) = from_e else {
        return note(world, format!("cannot march `{army_id}`: army has no land"));
    };
    if from_e == target_e {
        return note(world, format!(
            "cannot march `{army_id}`: the army is already on {target_id}"
        ));
    }

    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
    let from_name = world
        .get::<LandName>(from_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let to_name = world
        .get::<LandName>(target_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| target_id.to_string());

    // Spawn the marching entity. The three relationships
    // (`MarchingArmy` / `MarchingFromLand` / `MarchingToLand`) hit Bevy's
    // hooks and auto-fill the reverses on the army and both lands
    // synchronously. Dates stay empty until the daily tick activates the
    // marching.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Marching,
            MarchingArmy(army_e),
            MarchingFromLand(from_e),
            MarchingToLand(target_e),
            MarchingStatus::Scheduled,
            MarchingBeginDate(None),
            MarchingArrivedDate(None),
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    // ponytail: no `ArmyMarching` insertion here — the daily tick does
    // that when activating the scheduled marching. Until then the army is
    // still Idle (or already Marching on an earlier marching) and the
    // queued marching is just sitting in the army's `ArmyHasMarching`
    // collection waiting for the army to be on the matching source land.

    note(
        world,
        format!("queued {army_name} march: {from_name} → {to_name} (14 days)"),
    );
}

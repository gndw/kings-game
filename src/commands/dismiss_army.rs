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

use super::core::{distribute_levy_back, Choice, Command, MenuItem, note};
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyName};
use crate::ecs::kingdom::KingdomHold;
use crate::ecs::{CharacterLeads, KingdomHasArmies, Registry, StringId};
use crate::events::{BuildingUpdateKind, OnArmyDismiss, OnBuildingUpdated};
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

/// `(army_instance_id, "<land>:<levy>")` for every army in every kingdom
/// the actor leads, in `CharacterLeads` order followed by `KingdomHasArmies`.
/// Multi-kingdom: the player can rule several kingdoms at once, so the army
/// list is the union across every kingdom they lead. Walks the relationship
/// targets via `world::get` so it stays a `&World` read.
fn armies_under(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(*kingdom_e) else {
            continue;
        };
        for army_e in kingdom_has_armies.iter() {
            let string_id = match world.get::<StringId>(army_e) {
                Some(s) => s.0.clone(),
                None => continue,
            };
            // For the label we need the land name (army → land → name) and the
            // levy count. Army→land is via `ArmyOnLand`; the levy is
            // `ArmyLevy`. Both reads are `world::get` so they stay `&World`.
            let army_on_land = match world.get::<crate::ecs::army::ArmyOnLand>(army_e) {
                Some(a) => a,
                None => continue,
            };
            let land_name = world
                .get::<crate::ecs::LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            out.push((string_id, format!("{land_name}: {levy}")));
        }
    }
    out
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
    // BEFORE the despawn — `distribute_levy_back` walks `LandHasBuildings`
    // on the kingdom's home land. The levy was raised from those pools,
    // so it returns there regardless of where the army ended up. The
    // returned list is the buildings that were actually raised before
    // (so the per-building `OnBuildingUpdated` only fires for real state
    // transitions, not the defensive flag flips for never-raised buildings).
    let dismissed = distribute_levy_back(world, kingdom_land_e, army_levy);

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
        format!(
            "dismissed {army_name} on {army_land_name} ({army_levy} levy returned to {kingdom_land_name})"
        ),
    );

    // Publish after despawn. Observers must not read components on
    // `army_e` (gone), only its former relationships — most cleanup work
    // is keyed on the entity id alone.
    world.trigger(OnArmyDismiss { army: army_e });
    // Per-building state event: each actually-raised building flipped its
    // `BuildingIsRaised` flag back to false.
    for b_e in dismissed {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: kingdom_land_e,
            kind: BuildingUpdateKind::Dismissed,
        });
    }
}
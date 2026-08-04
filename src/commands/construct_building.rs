//! The construct-building command: spawn a building instance on a land the
//! actor rules, paid from their treasury.
//!
//! All immutable reads happen in [`validate`] (against `&World`); all
//! `&mut World` happens in [`construct_building`], never tangled. On success it
//! spawns the same bundle [`crate::ecs::populate`] uses, so a built building is
//! indistinguishable from an authored one — and [`recompute_yields`] already
//! runs each `FixedUpdate`, so the new building's gold/levy flows next tick
//! with no wiring.
//!
//! [`recompute_yields`]: crate::updates::yields::recompute_yields

use super::core::{next_id, note};
use crate::ecs::{Building, BuildingOf, CharacterGold, HeldBy, Leads, OnLand, Registry, StringId};
use crate::resources::buildings::BuildingDefs;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

/// The validated go-ahead: the entities and numbers [`construct_building`]
/// mutates with.
struct Go {
    actor_e: Entity,
    land_e: Entity,
    price: u32,
    def_name: String,
}

/// Check the rules against a snapshot (`&World`): the def exists, the actor
/// rules the land (their kingdom — via [`Leads`] — equals the land's
/// [`HeldBy`]), and they can afford the `construction_price`. Returns the
/// go-ahead or a rejection reason.
fn validate(world: &World, actor: &str, land_id: &str, def_id: &str) -> Result<Go, String> {
    let registry = world.resource::<Registry>();
    let defs = world.resource::<BuildingDefs>();
    let def = defs
        .get(def_id)
        .ok_or_else(|| format!("unknown building `{def_id}`"))?;
    let actor_e = registry
        .get(actor)
        .ok_or_else(|| format!("unknown actor `{actor}`"))?;
    let land_e = registry
        .get(land_id)
        .ok_or_else(|| format!("no land `{land_id}`"))?;

    // Rule check: the actor leads the kingdom that holds the land.
    let actor_k = world.get::<Leads>(actor_e).map(|l| l.kingdom());
    let land_k = world.get::<HeldBy>(land_e).map(|h| h.0);
    if actor_k.is_none() || actor_k != land_k {
        return Err("you don't rule that land".into());
    }

    // Afford: no building into debt (boring default; flip to allow debt).
    let gold = world.get::<CharacterGold>(actor_e).map(|g| g.0).unwrap_or(0);
    if gold < def.construction_price as i64 {
        return Err(format!("need {} gold", def.construction_price));
    }

    Ok(Go {
        actor_e,
        land_e,
        price: def.construction_price,
        def_name: def.name.clone(),
    })
}

/// Construct `def_id` on `land_id` for `actor`. Validates, pays, spawns, and
/// logs. See the module docs for the rules.
pub(super) fn construct_building(
    world: &mut World,
    actor: &str,
    land_id: &str,
    def_id: &str,
) {
    let go = match validate(world, actor, land_id, def_id) {
        Ok(g) => g,
        Err(msg) => return note(world, format!("cannot build on {land_id}: {msg}")),
    };

    // Pay.
    if let Some(mut gold) = world.get_mut::<CharacterGold>(go.actor_e) {
        gold.0 -= go.price as i64;
    }

    // Spawn the instance. `recompute_yields` picks its gold/levy up next tick.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Building,
            BuildingOf(def_id.to_string()),
            OnLand(go.land_e),
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    note(world, format!("built {} on {}", go.def_name, land_id));
}

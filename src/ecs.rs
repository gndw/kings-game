//! The simulation's entity-component model and the world it lives in.
//!
//! The entities live directly in Bevy's App world — [`Ctx`](crate::ctx::Ctx)
//! holds only the session state that isn't an entity (rng, chronicles, the
//! player id, the current selection). Reads go through Bevy `Query` system
//! params (the UI) or `&mut World` free functions (sim logic, which mixes
//! component and resource access and so runs as exclusive systems).
//!
//! - **House** entities: [`StringId`], [`House`].
//! - **Character** entities: [`StringId`], [`Character`],
//!   [`HouseOf`], [`CharacterState`], maybe [`KingdomLedBy`].
//! - **Land** entities: [`StringId`], [`Land`], [`Built`].
//! - **Kingdom** entities: [`StringId`], [`Kingdom`], [`LedBy`],
//!   [`Seat`], [`Holds`].
//!
//! Building *definitions* are not entities — they are a read-only roster held
//! as the [`Buildings`](crate::resources::buildings::Buildings) resource; lands
//! keep the ids of what's built in [`Built`].
//!
//! Load-time [`Content`](crate::content::Content) /
//! [`State`](crate::state::State) (still `IndexMap`-based — the deserialization,
//! merge and reconcile targets) are consumed by [`populate`] once, in
//! [`Ctx::new_game`](crate::ctx::Ctx::new_game); afterwards they are gone and
//! the ECS is the whole world.
//!
//! Two invariants carried over from the `IndexMap` model:
//!
//! - **Every game entity carries a [`StringId`]** — the id its RON data and save
//!   address it by. The Rhai script ABI is string ids and does not change.
//! - **Read order is Bevy archetype order**, which within one archetype is spawn
//!   order. Each kind (houses, characters, lands, kingdoms) is a single
//!   archetype, so a `Query` over e.g. `(&StringId, &Land)` yields lands in the
//!   order [`populate`] spawned them.
//!
//! A [`Registry`] resource maps `StringId → Entity` for O(1) lookup, the role
//! the `IndexMap` keys once played. Reading the registry and then mutating the
//! entity it points at is the standard two-step: pull the (cheap, `Copy`)
//! `Entity` out, drop the borrow, then touch the entity.

use crate::content::Content;
use crate::state::State;
use bevy::ecs::world::World;
use bevy::prelude::{Component, Entity, Resource};
use rand::Rng;
use rand::seq::IteratorRandom;
use std::collections::HashMap;

// ===========================================================================
// Shared components
// ===========================================================================

/// The id an entity is known by in RON data and save files. Every game entity
/// has one; the Rhai surface (`ctx.gold("char-tywin")`, …) is built on it.
#[derive(Component, Debug, Clone)]
pub struct StringId(pub String);

impl StringId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `id → Entity`, for the O(1) lookup the `IndexMap` keys once gave. Held as a
/// resource on the [`World`].
///
/// Reading the registry and then mutating an entity it points at is the
/// standard two-step dance: pull the (cheap, `Copy`) `Entity` out of the
/// registry, drop the borrow, then touch the entity.
#[derive(Resource, Default, Debug)]
pub struct Registry {
    pub by_id: HashMap<String, Entity>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `id` → `entity`, overwriting any earlier mapping for that id.
    /// Same replace-in-place rule as `IndexMap::insert`.
    pub fn insert(&mut self, id: String, entity: Entity) {
        self.by_id.insert(id, entity);
    }

    /// The entity known by `id`, if any.
    pub fn get(&self, id: &str) -> Option<Entity> {
        self.by_id.get(id).copied()
    }
}

// ===========================================================================
// Per-entity components
// ===========================================================================

/// A family. Characters belong to one; kingdoms are ruled through them.
#[derive(Component, Debug, Clone)]
pub struct House {
    pub name: String,
}

/// The read-only half of a character: their name. Their house is [`HouseOf`];
/// their treasury and levy are [`CharacterState`].
#[derive(Component, Debug, Clone)]
pub struct Character {
    pub name: String,
}

/// Which house a character belongs to. Points at a [`House`] entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct HouseOf(pub Entity);

/// The mutable half of a character: age, treasury, troops, monthly yield. All
/// fields `Copy`, so reads hand back a cheap snapshot.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterState {
    pub age: u32,
    pub gold: i64,
    pub levy: u64,
    pub gold_yield: i64,
}

/// Points at the [`Kingdom`] entity a character leads — the reverse of
/// [`LedBy`], for O(1) character→kingdom lookup.
// ponytail: one kingdom per character; a leader of two keeps only the last.
// Add KingdomLeads(Vec) if a character ever rules more than one.
#[derive(Component, Debug, Clone, Copy)]
pub struct KingdomLedBy(pub Entity);

/// One land's read-only geometry: outline and seat of power.
#[derive(Component, Debug, Clone)]
pub struct Land {
    pub name: String,
    pub borders: Vec<(f64, f64)>,
    pub holding: (f64, f64),
}

/// What stands in a land: the ids of the buildings built there. State, not
/// content — it changes in play and belongs in a save. Looked up against the
/// [`Buildings`](crate::resources::buildings::Buildings) resource to render.
#[derive(Component, Debug, Clone, Default)]
pub struct Built(pub Vec<String>);

/// Tags a kingdom entity. A kingdom is otherwise just its relations.
#[derive(Component, Debug, Clone, Copy)]
pub struct Kingdom;

/// The character who rules a kingdom. Points at a [`Character`] entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct LedBy(pub Entity);

/// The capital land of a kingdom. Points at a [`Land`] entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct Seat(pub Entity);

/// The lands a kingdom holds. Entities point at [`Land`]s.
#[derive(Component, Debug, Clone, Default)]
pub struct Holds(pub Vec<Entity>);

// ===========================================================================
// Building the world from content + state
// ===========================================================================

/// Build the entity world from merged, reconciled content and state. Called
/// once from [`Ctx::new_game`](crate::ctx::Ctx::new_game); afterwards content
/// and state are gone.
///
/// Spawn order is leaves-first — houses, then characters (which point at
/// houses), then lands, then kingdoms (which point at characters and lands) —
/// so a relation always resolves to an entity that already exists.
/// [`reconcile`](crate::state::reconcile) has already pruned every dangling
/// reference, so the `filter_map`s here only guard against logic errors, not
/// bad data. The building roster leaves as the [`Buildings`] resource rather
/// than entities.
pub fn populate(world: &mut World, content: Content, mut state: State) {
    world.insert_resource(Registry::new());
    world.insert_resource(content.buildings);

    // Houses.
    for (id, h) in content.houses.into_iter() {
        let eid = world
            .spawn((StringId(id.clone()), House { name: h.name }))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Characters: content half joined with state half by id.
    for (id, c) in content.characters.into_iter() {
        let st = state.characters.shift_remove(&id).unwrap_or_default();
        let house_e = world.resource::<Registry>().get(&c.house_id);
        let eid = {
            let mut ec = world.spawn((
                StringId(id.clone()),
                Character { name: c.name },
                CharacterState {
                    age: st.age,
                    gold: st.gold,
                    levy: st.levy,
                    gold_yield: st.gold_yield,
                },
            ));
            if let Some(he) = house_e {
                ec.insert(HouseOf(he));
            }
            ec.id()
        };
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Lands: content geometry + state's building list (the ids, kept as-is).
    for (id, l) in content.lands.into_iter() {
        let lst = state.lands.shift_remove(&id).unwrap_or_default();
        let eid = world
            .spawn((
                StringId(id.clone()),
                Land {
                    name: l.name,
                    borders: l.borders,
                    holding: l.holding,
                },
                Built(lst.building_ids),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Kingdoms: state-only. Their leader, seat and holdings resolve to the
    // characters and lands spawned above.
    for (id, k) in state.kingdoms.into_iter() {
        let leader = world.resource::<Registry>().get(&k.leader_character_id);
        let seat = world.resource::<Registry>().get(&k.seat_land_id);
        let holds: Vec<Entity> = k
            .land_ids
            .iter()
            .filter_map(|lid| world.resource::<Registry>().get(lid))
            .collect();
        let eid = {
            let mut ec = world.spawn((StringId(id.clone()), Kingdom));
            if let Some(le) = leader {
                ec.insert(LedBy(le));
            }
            if let Some(se) = seat {
                ec.insert(Seat(se));
            }
            ec.insert(Holds(holds));
            ec.id()
        };
        // Reverse of LedBy on the leader, for O(1) character→kingdom lookup.
        if let Some(le) = leader {
            world.entity_mut(le).insert(KingdomLedBy(eid));
        }
        world.resource_mut::<Registry>().insert(id, eid);
    }
}

/// A random land's id, or `None` when there are no lands. Drawn from the seeded
/// RNG so it replays.
pub fn random_land_id(world: &World, rng: &mut impl Rng) -> Option<String> {
    world
        .iter_entities()
        .filter(|e| e.get::<Land>().is_some())
        .choose(rng)
        .and_then(|e| e.get::<StringId>().map(|s| s.0.clone()))
}

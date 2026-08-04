//! The shared spine of the ECS: the [`StringId`](super::StringId) every entity
//! carries, the [`Registry`](super::Registry) that maps ids to entities for O(1)
//! lookup, and [`populate`](super::populate), which builds the world once from
//! [`Content`](crate::content::Content) — the merged definitions with the
//! starting state already overlaid.

use crate::content::Content;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::world::World;
use bevy::prelude::{Component, Entity, Reflect, Resource};
use std::collections::HashMap;

use super::character::{
    Character, CharacterAge, CharacterGold, CharacterGoldYield, CharacterLevy, CharacterName,
    HouseOf,
};
use super::house::{House, HouseName};
use super::kingdom::{Kingdom, LedBy, Seat};
use super::land::{Built, HeldBy, Land, LandBorders, LandHolding, LandName};

/// The id an entity is known by in RON data and save files. Every game entity
/// has one; the scripting surface (`ctx.gold("char-tywin")`, …) is built on it.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
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

/// Build the entity world from merged, reconciled content (state already
/// overlaid). Called once from [`Ctx::new_game`](crate::ctx::Ctx::new_game);
/// afterwards the content is gone.
///
/// Spawn order is leaves-first — houses, then characters (which point at
/// houses), then lands, then kingdoms (which point at characters and lands) —
/// so a relation always resolves to an entity that already exists.
/// [`reconcile`](crate::state::reconcile) has already pruned every dangling
/// reference, so the `filter_map`s here only guard against logic errors, not
/// bad data. The building roster leaves as the
/// [`Buildings`](crate::resources::buildings::Buildings) resource rather than
/// entities.
pub fn populate(world: &mut World, content: Content) {
    world.insert_resource(Registry::new());
    world.insert_resource(content.buildings);

    // Houses.
    for (id, h) in content.houses.into_iter() {
        let eid = world
            .spawn((StringId(id.clone()), House, HouseName(h.name)))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Characters: one struct holds both definition and state now.
    for (id, c) in content.characters.into_iter() {
        let house_e = world.resource::<Registry>().get(&c.house_id);
        let eid = {
            let mut ec = world.spawn((
                StringId(id.clone()),
                Character,
                CharacterName(c.name),
                CharacterAge(c.age),
                CharacterGold(c.gold),
                CharacterLevy(c.levy),
                CharacterGoldYield(c.gold_yield),
            ));
            if let Some(he) = house_e {
                ec.insert(HouseOf(he));
            }
            ec.id()
        };
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Lands: geometry + the buildings that stand on them.
    for (id, l) in content.lands.into_iter() {
        let eid = world
            .spawn((
                StringId(id.clone()),
                Land,
                LandName(l.name),
                LandBorders(l.borders),
                LandHolding(l.holding),
                Built(l.building_ids),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Kingdoms: state-only. Their leader, seat and holdings resolve to the
    // characters and lands spawned above.
    for (id, k) in content.kingdoms.into_iter() {
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
            ec.id()
        };
        // Each land declares the kingdom holding it; `Holds` on the kingdom is
        // auto-maintained by the relationship hook.
        for &le in &holds {
            world.entity_mut(le).insert(HeldBy(eid));
        }
        // `LedBy` is a Bevy relationship: its hook auto-maintains `Leads` on the
        // leader, so there is no manual reverse insert here.
        world.resource_mut::<Registry>().insert(id, eid);
    }
}

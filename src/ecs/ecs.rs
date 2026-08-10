//! The shared spine of the ECS: the [`StringId`](super::StringId) every entity
//! carries, the [`Registry`](super::Registry) that maps ids to entities for O(1)
//! lookup, and [`populate`](super::populate), which builds the world once from
//! [`Content`](crate::content::Content) — the merged definitions with the
//! starting state already overlaid.

use crate::content::Content;
use crate::resources::buildings::BuildingDefs;
use bevy::ecs::world::World;
use bevy::prelude::{Component, Entity, Resource};
use std::collections::HashMap;

use super::building::{Building, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand};
use super::character::{
    Character, CharacterDateOfBirth, CharacterGold, CharacterGoldYield, CharacterLevy,
    CharacterName, CharacterOfHouse,
};
use super::courtier::{Courtier, CourtierOfCharacter, CourtierOfKingdom};
use super::house::{House, HouseName};
use super::kingdom::{Kingdom, KingdomHold, KingdomLedBy};
use super::land::{Land, LandBorders, LandHolding, LandName};
use super::road::{Road, RoadBetweenLands, RoadDistanceDays, RoadPoints};

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

/// Build the entity world from merged, reconciled content (state already
/// overlaid). Called once from [`Ctx::new_game`](crate::ctx::Ctx::new_game);
/// afterwards the content is gone.
///
/// Spawn order is leaves-first — houses, then characters (which point at
/// houses), then lands, then the buildings standing on them, then kingdoms
/// (which point at characters and lands) — so a relation always resolves to an
/// entity that already exists. [`reconcile`](crate::state::reconcile) has
/// already pruned every dangling reference, so the `filter_map`s here only guard
/// against logic errors, not bad data. The building *definition* roster leaves
/// as the [`BuildingDefs`](crate::resources::buildings::BuildingDefs) resource;
/// each building *instance* becomes an entity.
pub fn populate(world: &mut World, content: Content) {
    world.insert_resource(Registry::new());
    world.insert_resource(content.building_defs);

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
                CharacterDateOfBirth(c.dob),
                CharacterGold(c.gold),
                CharacterLevy(c.levy),
                CharacterGoldYield(c.gold_yield),
            ));
            if let Some(he) = house_e {
                ec.insert(CharacterOfHouse(he));
            }
            ec.id()
        };
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Lands: pure geometry now — buildings stand as their own entities.
    for (id, l) in content.lands.into_iter() {
        let eid = world
            .spawn((
                StringId(id.clone()),
                Land,
                LandName(l.name),
                LandBorders(l.borders),
                LandHolding(l.holding),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Buildings: one entity per built instance. Spawned after lands so
    // `BuildingOnLand` resolves to an entity that already exists; the land's
    // `LandHasBuildings` is auto-maintained by the relationship hook. The
    // per-instance status comes from the state overlay (defaults to
    // `BuildingStatus::Active`); the construction-date is only meaningful on
    // `BUILDING` instances and is set by `ConstructBuilding` at runtime.
    // `BuildingLevy` starts at the def's `levy` (full pool); `BuildingIsRaised`
    // starts as `false` (no army in the field yet).
    for (id, b) in content.buildings.into_iter() {
        let Some(land_e) = world.resource::<Registry>().get(&b.land_id) else {
            continue;
        };
        let def_levy = world
            .resource::<BuildingDefs>()
            .get(&b.def_id)
            .map(|d| d.levy)
            .unwrap_or(0);
        let eid = world
            .spawn((
                StringId(id.clone()),
                Building,
                BuildingOf(b.def_id),
                BuildingOnLand(land_e),
                b.status,
                BuildingLevy(def_levy),
                BuildingIsRaised(false),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Kingdoms: state-only. Their leader and single land resolve to the
    // characters and lands spawned above.
    for (id, k) in content.kingdoms.into_iter() {
        let leader = world.resource::<Registry>().get(&k.leader_character_id);
        let land = world.resource::<Registry>().get(&k.land_id);
        let eid = {
            let mut ec = world.spawn((StringId(id.clone()), Kingdom));
            if let Some(le) = leader {
                ec.insert(KingdomLedBy(le));
            }
            // The kingdom declares the land it holds; `LandHeldBy` on the land
            // is auto-maintained by the relationship hook.
            if let Some(le) = land {
                ec.insert(KingdomHold(le));
            }
            ec.id()
        };
        // `KingdomLedBy` is a Bevy relationship: its hook auto-maintains
        // `CharacterLeads` on the leader, so there is no manual reverse insert
        // here.
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Court appointments resolve both characters and kingdoms spawned above.
    for (id, c) in content.courtiers {
        let character = world.resource::<Registry>().get(&c.character_id);
        let kingdom = world.resource::<Registry>().get(&c.kingdom_id);
        let (Some(character), Some(kingdom)) = (character, kingdom) else {
            continue;
        };
        let eid = world
            .spawn((
                StringId(id.clone()),
                Courtier,
                c.courtier_type,
                CourtierOfCharacter(character),
                CourtierOfKingdom(kingdom),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Roads: definition-only, baked once. Spawned after lands so the id→entity
    // lookup in the registry resolves. `validate` already guarantees the two
    // land ids exist, so this only guards against logic errors, not bad data.
    for (id, r) in content.roads {
        let lands: Vec<Entity> = {
            let registry = world.resource::<Registry>();
            r.between_land_ids
                .iter()
                .filter_map(|lid| registry.get(lid))
                .collect()
        };
        if lands.len() != 2 {
            continue;
        }
        let eid = world
            .spawn((
                StringId(id.clone()),
                Road,
                RoadPoints(r.points),
                RoadBetweenLands(lands),
                RoadDistanceDays(r.distance_days),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }
}

//! The shared spine: `StringId` every entity carries, `Registry` for O(1) id→Entity,
//! and `populate` which builds the world once from `Content`.

use crate::content::Content;
use crate::resources::buildings::BuildingDefs;
use bevy::ecs::world::World;
use bevy::prelude::{Component, Entity, Resource};
use std::collections::HashMap;

use super::building::{Building, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand};
use super::character::{
    Character, CharacterDateOfBirth, CharacterGold, CharacterGoldYield, CharacterHasFather,
    CharacterHasHusband, CharacterHasMother, CharacterLevy, CharacterName, CharacterOfHouse,
};
use super::courtier::{Courtier, CourtierOfCharacter, CourtierOfKingdom};
use super::house::{House, HouseName};
use super::kingdom::{Kingdom, KingdomHold, KingdomLedBy};
use super::land::{Land, LandBorders, LandHolding, LandName};
use super::road::{Road, RoadBetweenLands, RoadDistanceDays, RoadPoints};

/// The id an entity is known by in RON data and save files.
#[derive(Component, Debug, Clone)]
pub struct StringId(pub String);

impl StringId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `id → Entity`. The standard two-step: pull the `Copy` `Entity` out, drop the borrow, then touch.
#[derive(Resource, Default, Debug)]
pub struct Registry {
    pub by_id: HashMap<String, Entity>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `id` → `entity`, overwriting any earlier mapping for that id.
    pub fn insert(&mut self, id: String, entity: Entity) {
        self.by_id.insert(id, entity);
    }

    pub fn get(&self, id: &str) -> Option<Entity> {
        self.by_id.get(id).copied()
    }
}

/// Build the entity world from merged, reconciled content. Spawn order is
/// leaves-first so every relation resolves to an entity that already exists.
pub fn populate(world: &mut World, content: Content) {
    world.insert_resource(Registry::new());
    world.insert_resource(content.building_defs);

    for (id, h) in content.houses.into_iter() {
        let eid = world.spawn((StringId(id.clone()), House, HouseName(h.name))).id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

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
                c.sex,
            ));
            if let Some(he) = house_e {
                ec.insert(CharacterOfHouse(he));
            }
            ec.id()
        };
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // Wire up family ties after every character exists. `validate` has already
    // confirmed all ids resolve; `filter_map`s here would silently drop a
    // malformed entry, which the validator would have caught.
    for (_, f) in content.families.into_iter() {
        use crate::content::FamilyType;
        let registry = world.resource::<Registry>();
        let lookup = |id: &str| registry.get(id);
        match f.family_type {
            FamilyType::Family => {
                let child = lookup(&f.child_character_id);
                let father = lookup(&f.father_character_id);
                let mother = lookup(&f.mother_character_id);
                if let (Some(child), Some(father)) = (child, father) {
                    world.entity_mut(child).insert(CharacterHasFather(father));
                }
                if let (Some(child), Some(mother)) = (child, mother) {
                    world.entity_mut(child).insert(CharacterHasMother(mother));
                }
            }
            FamilyType::Marriage => {
                let husband = lookup(&f.husband_character_id);
                let wife = lookup(&f.wife_character_id);
                if let (Some(husband), Some(wife)) = (husband, wife) {
                    // Set the relationship on one side; Bevy's hook maintains the reverse.
                    world.entity_mut(wife).insert(CharacterHasHusband(husband));
                }
            }
        }
    }

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

    for (id, k) in content.kingdoms.into_iter() {
        let leader = world.resource::<Registry>().get(&k.leader_character_id);
        let land = world.resource::<Registry>().get(&k.land_id);
        let eid = {
            let mut ec = world.spawn((StringId(id.clone()), Kingdom));
            if let Some(le) = leader {
                ec.insert(KingdomLedBy(le));
            }
            if let Some(le) = land {
                ec.insert(KingdomHold(le));
            }
            ec.id()
        };
        world.resource_mut::<Registry>().insert(id, eid);
    }

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

//! What a kind of building is, from `building_definitions.ron`. An ECS resource
//! holding the read-only roster, seeded into the world in
//! [`crate::ecs::populate`].
//!
//! The *definitions* live here (one entry per building kind, shared across every
//! instance); the *instances* standing in lands are ECS entities
//! (see [`crate::ecs::building`]). A building entity carries the id of its
//! definition in [`BuildingOf`](crate::ecs::BuildingOf); yields and the
//! buildings panel look the stats up here.

use bevy::prelude::Resource;
use indexmap::IndexMap;
use serde::Deserialize;

/// One building definition. Military ones cost `gold_upkeep` and add `levy`
/// troops; civil ones earn `gold_profit`. One gold field set, never both — the
/// other stays 0. `construction_price` is the one-off gold cost to build one;
/// `construction_time` is how many in-game days it takes (the new building
/// spawns as `BuildingStatus::Building` and flips to `Active` once the date
/// advances past the start date + this value).
///
/// `levy_rate` is the per-month replenishment the building contributes to
/// armies raised on its land (military only — defaults to 0 on civil kinds).
/// Read by future replenishment code, not yet consumed: this commit just
/// declares the field.
///
/// `fort_level` is the fortification tier of the building — `0` for
/// non-fortified kinds (the default; existing buildings don't specify it)
/// and a positive value for fortified kinds like `Castle`. The field exists
/// on the roster so future war-resolution code (siege modifiers, conquest
/// defense) can read it; no sim path consumes it yet.
#[derive(Clone, Debug, Deserialize)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub gold_profit: u32,
    #[serde(default)]
    pub gold_upkeep: u32,
    #[serde(default)]
    pub levy: u32,
    #[serde(default)]
    pub levy_rate: u32,
    #[serde(default)]
    pub fort_level: u32,
    #[serde(default)]
    pub construction_price: u32,
    #[serde(default)]
    pub construction_time: u32,
}

/// The roster: ID-keyed for O(1) lookup, insertion-ordered for deterministic
/// iteration — the same rule as the rest of `Content`.
#[derive(Clone, Debug, Default, Resource)]
pub struct BuildingDefs(pub IndexMap<String, BuildingDef>);

impl BuildingDefs {
    pub fn get(&self, id: &str) -> Option<&BuildingDef> {
        self.0.get(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }
}

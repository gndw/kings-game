//! Building definitions (one entry per kind) from `building_definitions.ron`.
//! Instances are ECS entities; this is the read-only stat roster.

use bevy::prelude::Resource;
use indexmap::IndexMap;
use serde::Deserialize;

/// One building definition. Military kinds cost `gold_upkeep` and add `levy`;
/// civil ones earn `gold_profit`. Construction cost + time apply to both.
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

/// The roster: ID-keyed for O(1) lookup, insertion-ordered for deterministic iteration.
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

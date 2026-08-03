//! What a holding can raise, from `buildings.ron`. An ECS resource holding the
//! roster, seeded into the world in [`crate::ecs::populate`].
//!
//! A read-only definition roster is a resource, not entities — the same call
//! [`Calendar`](crate::resources::calendar::Calendar) already answers. Lands
//! hold the ids of what's built via `ecs::Built`; yields and the legend look
//! the id up here.

use bevy::prelude::Resource;
use indexmap::IndexMap;
use serde::Deserialize;

/// One building definition. Military ones cost `gold_upkeep` and add `levy`
/// troops; civil ones earn `gold_profit`. One gold field set, never both — the
/// other stays 0.
#[derive(Clone, Debug, Deserialize)]
pub struct Building {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub gold_profit: u32,
    #[serde(default)]
    pub gold_upkeep: u32,
    #[serde(default)]
    pub levy: u32,
}

/// The roster: ID-keyed for O(1) lookup, insertion-ordered for deterministic
/// iteration — the same rule as the rest of `Content`.
#[derive(Clone, Debug, Default, Resource)]
pub struct Buildings(pub IndexMap<String, Building>);

impl Buildings {
    pub fn get(&self, id: &str) -> Option<&Building> {
        self.0.get(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }
}

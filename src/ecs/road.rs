//! Road entities: connections between two lands. Definition-only — built once
//! at populate time, never edited.

use super::marching::MarchingOnRoad;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// Tags a road entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct Road;

/// The road's polyline. One point per vertex.
#[derive(Component, Debug, Clone)]
pub struct RoadPoints(pub Vec<(f64, f64)>);

/// The two lands a road joins. Length is exactly 2; content validation enforces this.
#[derive(Component, Debug, Clone)]
pub struct RoadBetweenLands(pub Vec<Entity>);

/// Days an army spends marching this road — the full duration of a marching.
/// Authored per road; always ≥ 1 (`validate` rejects 0).
#[derive(Component, Debug, Clone, Copy)]
pub struct RoadDistanceDays(pub u32);

/// Marchings travelling this road — auto-maintained reverse of `MarchingOnRoad`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingOnRoad)]
pub struct RoadHasMarchings(Vec<Entity>);

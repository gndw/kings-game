//! Road entities: connections between two lands.
//!
//! A road carries the [`Road`] marker, its polyline in [`RoadPoints`], and
//! the two lands it joins in [`RoadBetweenLands`]. Roads are definition-only
//! (built once at populate time from the [`Road`](crate::content::Road)
//! content records, never edited) — so [`RoadBetweenLands`] is a plain
//! `Vec<Entity>` rather than a Bevy relationship. The reverse collection is
//! not maintained; gameplay code that needs to walk roads from a land reads
//! [`RoadPoints`] / [`RoadBetweenLands`] directly.
//!
//! Visual: the dashed line is drawn by
//! [`road_graphic`](crate::map::components::road_graphic) using Bevy's
//! retained-gizmo [`Gizmo`](bevy::prelude::Gizmo) component — one per road,
//! spawned at startup, no per-frame update.

use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// Tags a road entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct Road;

/// The road's polyline. One point per vertex; consecutive points are joined
/// by a (dashed) line.
#[derive(Component, Debug, Clone)]
pub struct RoadPoints(pub Vec<(f64, f64)>);

/// The two lands a road joins, in west-to-east order. Length is exactly 2;
/// content validation enforces this before populate.
#[derive(Component, Debug, Clone)]
pub struct RoadBetweenLands(pub Vec<Entity>);

//! Road entities: connections between two lands.
//!
//! A road carries the [`Road`] marker, its polyline in [`RoadPoints`], the
//! two lands it joins in [`RoadBetweenLands`], and its marching cost in
//! [`RoadDistanceDays`]. Roads are definition-only
//! (built once at populate time from the [`Road`](crate::content::Road)
//! content records, never edited) — so [`RoadBetweenLands`] is a plain
//! `Vec<Entity>` rather than a Bevy relationship. The land-side reverse is
//! not maintained; gameplay code that needs to walk roads from a land reads
//! [`RoadPoints`] / [`RoadBetweenLands`] directly (the marching command's
//! route search does exactly that).
//!
//! The one live link on a road is [`RoadHasMarchings`] — the auto-maintained
//! reverse of [`MarchingOnRoad`](super::marching::MarchingOnRoad), listing
//! the marchings currently travelling this road. That one *is* a Bevy
//! relationship because marchings come and go at run time; the road's own
//! definition data still never changes.
//!
//! Visual: the dashed line is drawn by
//! [`road_graphic`](crate::map::components::road_graphic) using Bevy's
//! retained-gizmo [`Gizmo`](bevy::prelude::Gizmo) component — one per road,
//! spawned at startup, no per-frame update.

use super::marching::MarchingOnRoad;
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

/// Days an army spends marching this road — the full duration of a marching,
/// since one marching entity covers one road. Read by
/// [`road_days`](crate::game::marching::road_days), which the daily tick uses
/// to set a marching's arrived date and the marching command uses to total a
/// route. Authored per road in mod data (the base mod scales it off polyline
/// length, longest road = 30 days), so terrain can be priced independently of
/// how the road is drawn. Always ≥ 1: [`validate`](crate::content::validate)
/// rejects 0, which would let an army arrive the day it left.
#[derive(Component, Debug, Clone, Copy)]
pub struct RoadDistanceDays(pub u32);

/// The marchings travelling this road — the auto-maintained reverse of
/// [`MarchingOnRoad`](super::marching::MarchingOnRoad). Read-only: set
/// `MarchingOnRoad` on each marching and Bevy's hook keeps this in sync.
/// Holds both the scheduled and the on-route marchings on the road (a
/// multi-hop order spawns one marching per road, so only the hop the army
/// is actually walking is `OnRoute`).
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingOnRoad)]
pub struct RoadHasMarchings(Vec<Entity>);

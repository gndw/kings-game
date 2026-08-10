//! Visual marker for a road: a dashed polyline connecting the two lands.
//!
//! Lifecycle: [`startup`] spawns one [`RoadGraphic`] marker per road,
//! back-referenced to its road entity via [`UIWithRoad`]. [`update`] walks
//! every marker each frame, reads the road's [`RoadPoints`](crate::ecs::RoadPoints)
//! and draws the dashed polyline through a dedicated [`RoadGizmoConfigGroup`]
//! (the dash style is baked into the config — `Gizmos` has no per-call
//! style).
//!
//! Visual-only — lifecycle is event-free.

use super::common::UIWithRoad;
use bevy::color::Srgba;
use bevy::prelude::*;

/// Marker on the entity that drives the dashed road draw. One per road;
/// the per-road polyline lives on the [`RoadPoints`](crate::ecs::RoadPoints)
/// component of the back-reffed road entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct RoadGraphic;

/// Gizmo config group dedicated to the road outline. `GizmoLineStyle::Dashed`
/// is on the config — `Gizmos` has no per-call style — so all road draws
/// share this group. Registered once in `main` via `App::insert_gizmo_config`.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct RoadGizmoConfigGroup;

/// Dash gap in units of the line width — Bevy's `GizmoLineStyle::Dashed`
/// scales both the visible stroke and the gap by the gizmo's `width`. Public
/// because `main` reads these to register the gizmo config group — the
/// binary is a separate crate from the library, so `pub(crate)` isn't
/// enough.
pub const DASH_GAP_SCALE: f32 = 2.0;
/// Dash stroke in units of the line width.
pub const DASH_LINE_SCALE: f32 = 4.0;
/// Road line colour. Same warm brown as the holding-icon castle so roads
/// read as part of the same cartographic family.
const ROAD_COLOR: Srgba = Srgba::new(0.0, 0.0, 0.0, 0.3);

/// Spawn one [`RoadGraphic`] marker per road. `update` then drives the draw
/// every frame.
pub fn startup(
    mut commands: Commands,
    roads: Query<Entity, With<crate::ecs::Road>>,
) {
    for road_e in &roads {
        commands.spawn((RoadGraphic, UIWithRoad(road_e)));
    }
}

/// Per-frame dashed polyline draw for every road. Read in PostUpdate, after
/// the land outlines, so the road visually sits on top of the land fill but
/// under the holding castle / army sword drawn after it.
pub fn update(
    icons: Query<&UIWithRoad, With<RoadGraphic>>,
    roads: Query<&crate::ecs::RoadPoints, With<crate::ecs::Road>>,
    mut road_gizmos: Gizmos<RoadGizmoConfigGroup>,
) {
    for ui in &icons {
        let Ok(points) = roads.get(ui.0) else { continue };
        road_gizmos.linestrip_2d(
            points.0.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)),
            ROAD_COLOR,
        );
    }
}

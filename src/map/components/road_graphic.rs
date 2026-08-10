//! Visual marker for a road: a dashed polyline connecting the two lands.
//!
//! Lifecycle: [`startup`] spawns one [`RoadGraphic`] marker per road,
//! back-referenced to its road entity via [`UIWithRoad`]. [`update`] walks
//! every marker each frame, reads the road's [`RoadPoints`]
//! and draws the dashed polyline through a dedicated [`RoadGizmoConfigGroup`]
//! (the dash style is baked into the config — `Gizmos` has no per-call
//! style).
//!
//! The line's colour reports marching traffic, read off the road's
//! [`RoadHasMarchings`]: green while an army is actually walking it, gray
//! while a march is only queued on it, and the default otherwise. See
//! [`road_color`].
//!
//! Visual-only — lifecycle is event-free.

use super::common::UIWithRoad;
use crate::ecs::marching::MarchingStatus;
use crate::ecs::road::{Road, RoadHasMarchings, RoadPoints};
use bevy::color::Srgba;
use bevy::prelude::*;

/// Marker on the entity that drives the dashed road draw. One per road;
/// the per-road polyline lives on the [`RoadPoints`]
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
/// Road line colour while a march is queued on the road but no army is
/// walking it yet (`MarchingStatus::Scheduled`) — css gray at half alpha.
/// Reads as "spoken for" without pulling the eye like the active colour.
const ROAD_SCHEDULED_COLOR: Srgba = Srgba::new(0.5, 0.5, 0.5, 0.5);
/// Road line colour while an army is on the road right now
/// (`MarchingStatus::OnRoute`) — css green at half alpha.
const ROAD_ON_ROUTE_COLOR: Srgba = Srgba::new(0.0, 0.5, 0.0, 0.5);

/// Spawn one [`RoadGraphic`] marker per road. `update` then drives the draw
/// every frame.
pub fn startup(
    mut commands: Commands,
    roads: Query<Entity, With<Road>>,
) {
    for road_e in &roads {
        commands.spawn((RoadGraphic, UIWithRoad(road_e)));
    }
}

/// The colour to draw a road in, from the marchings currently on it.
///
/// A road can carry several marchings at once — other armies' marches, and
/// the queued hops of routes that pass through it — so the states are ranked
/// rather than counted: any army actually on the road wins (green), else a
/// queued march (gray), else the default. `None`/empty is the common case;
/// Bevy drops the `RoadHasMarchings` target once the last marching on the
/// road is despawned.
fn road_color(
    road_has_marchings: Option<&RoadHasMarchings>,
    marchings: &Query<&MarchingStatus>,
) -> Srgba {
    let mut color = ROAD_COLOR;
    let Some(road_has_marchings) = road_has_marchings else {
        return color;
    };
    for marching_e in road_has_marchings.iter() {
        match marchings.get(marching_e) {
            // Nothing outranks an army on the road; stop looking.
            Ok(MarchingStatus::OnRoute) => return ROAD_ON_ROUTE_COLOR,
            Ok(MarchingStatus::Scheduled) => color = ROAD_SCHEDULED_COLOR,
            Err(_) => {}
        }
    }
    color
}

/// Per-frame dashed polyline draw for every road, coloured by its marching
/// traffic ([`road_color`]). Read in PostUpdate, after
/// the land outlines, so the road visually sits on top of the land fill but
/// under the holding castle / army sword drawn after it.
pub fn update(
    icons: Query<&UIWithRoad, With<RoadGraphic>>,
    roads: Query<(&RoadPoints, Option<&RoadHasMarchings>), With<Road>>,
    marchings: Query<&MarchingStatus>,
    mut road_gizmos: Gizmos<RoadGizmoConfigGroup>,
) {
    for ui in &icons {
        let Ok((points, road_has_marchings)) = roads.get(ui.0) else { continue };
        road_gizmos.linestrip_2d(
            points.0.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)),
            road_color(road_has_marchings, &marchings),
        );
    }
}

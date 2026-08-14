//! Visual marker for a road: a dashed polyline connecting the two lands.
//!
//! Colour reports marching traffic: green while an army is walking it, gray
//! while a march is queued, default otherwise.

use super::common::UIWithRoad;
use crate::ecs::marching::MarchingStatus;
use crate::ecs::road::{Road, RoadHasMarchings, RoadPoints};
use bevy::color::Srgba;
use bevy::prelude::*;

/// Marker on the entity that drives the dashed road draw.
#[derive(Component, Debug, Clone, Copy)]
pub struct RoadGraphic;

/// Gizmo config group dedicated to the road outline — `GizmoLineStyle::Dashed`
/// is on the config because `Gizmos` has no per-call style.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct RoadGizmoConfigGroup;

pub const DASH_GAP_SCALE: f32 = 2.0;
pub const DASH_LINE_SCALE: f32 = 4.0;
const ROAD_COLOR: Srgba = Srgba::new(0.0, 0.0, 0.0, 0.3);
const ROAD_SCHEDULED_COLOR: Srgba = Srgba::new(0.5, 0.5, 0.5, 0.5);
const ROAD_ON_ROUTE_COLOR: Srgba = Srgba::new(0.0, 0.5, 0.0, 0.5);

/// Spawn one `RoadGraphic` marker per road.
pub fn startup(mut commands: Commands, roads: Query<Entity, With<Road>>) {
    for road_e in &roads {
        commands.spawn((RoadGraphic, UIWithRoad(road_e)));
    }
}

/// Colour to draw a road in, from the marchings currently on it. States are
/// ranked: an army on the road wins (green), else a queued march (gray).
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
            Ok(MarchingStatus::OnRoute) => return ROAD_ON_ROUTE_COLOR,
            Ok(MarchingStatus::Scheduled) => color = ROAD_SCHEDULED_COLOR,
            Err(_) => {}
        }
    }
    color
}

/// Per-frame dashed polyline draw for every road, coloured by its marching traffic.
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

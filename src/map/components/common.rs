//! Common icon components + shared drawing helpers used across the map's
//! icon kinds. Visual-only — placement and lifecycle are the icon's job.

use bevy::prelude::*;

/// Back-reference from an icon to the entity it represents. A per-frame
/// `update` system reads the target entity's position component through this
/// and copies the resulting position into the icon's `Transform`. Designed
/// to be generic — any icon that follows an entity (army, kingdom,
/// future character-on-land) can reuse it.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithArmy(pub Entity);

/// Back-reference from a per-land drawing entity (see
/// [`land_graphic`](super::land_graphic)) to the land it represents. The
/// `update` system reads the land's `LandBorders` + `StringId` through this
/// every frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithLand(pub Entity);

/// Back-reference from a per-road drawing entity (see
/// [`road_graphic`](super::road_graphic)) to the road it represents.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithRoad(pub Entity);

/// Gap between the horizontal lines that stand in for a polygon fill.
// ponytail: fixed world-space step — re-derive from the camera's current
// visible-size ratio if zoom gets coarse enough to show gaps.
const FILL_STEP: f64 = 3.0;

/// Wash a polygon in `color`. Gizmos draw lines only, so the fill is a
/// stack of horizontal scanlines: at each height, cross the polygon's
/// edges and join the crossings up in pairs. Handles concave polygons —
/// used for per-land fills in [`land_graphic`](super::land_graphic) and for
/// the world-border sea wash in
/// [`border_graphic`](super::border_graphic).
pub(crate) fn fill(gizmos: &mut Gizmos, poly: &[(f64, f64)], color: Color) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &(_, y) in poly {
        lo = lo.min(y);
        hi = hi.max(y);
    }
    let mut y = lo + FILL_STEP / 2.0;
    while y < hi {
        // Edges wrap around, so an outline that doesn't repeat its first
        // point still closes. A repeated one just yields a zero-length
        // edge.
        let mut xs: Vec<f64> = poly
            .iter()
            .zip(poly.iter().cycle().skip(1))
            .take(poly.len())
            .filter_map(|(&(xa, ya), &(xb, yb))| {
                ((ya > y) != (yb > y)).then(|| xa + (y - ya) / (yb - ya) * (xb - xa))
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        for span in xs.chunks_exact(2) {
            gizmos.line_2d(
                Vec2::new(span[0] as f32, y as f32),
                Vec2::new(span[1] as f32, y as f32),
                color,
            );
        }
        y += FILL_STEP;
    }
}

/// Back-reference from an icon to the kingdom it represents. Same shape as
/// [`UIWithArmy`]; kept separate so a single entity can wear multiple
/// `UIWith*` refs (an icon that follows both a kingdom and one of its
/// holdings, for example) without colliding on the field name.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithKingdom(pub Entity);

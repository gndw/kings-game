//! Common icon components + shared drawing helpers used across the map's icon kinds.

use bevy::prelude::*;

/// Back-reference from an icon to the entity it represents.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithArmy(pub Entity);

/// Back-reference from a per-land drawing entity to the land it represents.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithLand(pub Entity);

/// Back-reference from a per-road drawing entity to the road it represents.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithRoad(pub Entity);

const FILL_STEP: f64 = 3.0;

/// Wash a polygon in `color`. Gizmos draw lines only, so the fill is a stack
/// of horizontal scanlines; handles concave polygons.
pub(crate) fn fill(gizmos: &mut Gizmos, poly: &[(f64, f64)], color: Color) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &(_, y) in poly {
        lo = lo.min(y);
        hi = hi.max(y);
    }
    let mut y = lo + FILL_STEP / 2.0;
    while y < hi {
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

/// Back-reference from an icon to the kingdom it represents.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithKingdom(pub Entity);

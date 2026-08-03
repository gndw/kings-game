//! A waving pennant, planted on the selected holding.

use bevy::color::palettes::css;
use bevy::prelude::*;

const POLE: f32 = 28.0;
const CLOTH_W: f32 = 18.0;
const CLOTH_H: f32 = 10.0;
const ROWS: usize = 6; // even, so a row lands on the widest point
const SEGS: usize = 6;

/// Sideways offset of the cloth `t` (0 at the pole, 1 at the fly end) at `phase`
/// seconds. Slack grows with `t`, so the pole edge stays put and the tip whips.
fn wave(phase: f32, t: f32) -> f32 {
    (phase * 4.0 - t * 4.0).sin() * 2.0 * t
}

/// A small pennant on a pole, planted at world point `at`.
// ponytail: fixed world size like the holding circle; the camera never zooms.
pub fn draw(gizmos: &mut Gizmos, at: Vec2, phase: f32) {
    gizmos.line_2d(at, at + Vec2::Y * POLE, css::WHITE);
    // triangular pennant: full height at the pole, tapering to a point at the fly
    for i in 0..=ROWS {
        let v = i as f32 / ROWS as f32;
        let row = at.y + POLE - v * CLOTH_H;
        let len = CLOTH_W * (1.0 - 2.0 * (v - 0.5).abs());
        for j in 0..SEGS {
            let (t0, t1) = (j as f32 / SEGS as f32, (j + 1) as f32 / SEGS as f32);
            gizmos.line_2d(
                Vec2::new(at.x + t0 * len, row + wave(phase, t0 * len / CLOTH_W)),
                Vec2::new(at.x + t1 * len, row + wave(phase, t1 * len / CLOTH_W)),
                css::RED,
            );
        }
    }
}

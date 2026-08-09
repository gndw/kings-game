//! Visual marker for the map's frame: the world-border rectangle + a
//! scanline sea wash inside it. One [`BorderGraphic`] is spawned at startup
//! (via [`startup`]); the per-frame [`update`] redraws the rectangle and
//! fills the interior with [`common::fill`].
//!
//! Mirrors the `holding_icon` / `land_graphic` lifecycle pattern (one
//! startup-spawned marker, one per-frame update).
//!
//! Visual-only — lifecycle is event-free.

use super::common::fill;
use crate::resources::border::Border;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Marker on the entity that drives the world-border draw. There's only
/// ever one — the map has a single frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct BorderGraphic;

/// Spawn the single [`BorderGraphic`] entity. `update` then drives the draw
/// every frame.
pub fn startup(mut commands: Commands) {
    commands.spawn(BorderGraphic);
}

/// Per-frame world-border rectangle + sea wash inside it. Drawn before the
/// lands so the polygon outlines cover any border / sea wash that bleeds
/// outside their bounds.
pub fn update(mut gizmos: Gizmos, border: Res<Border>) {
    let b = &*border;
    gizmos.rect_2d(
        Isometry2d::from_xy(((b.x0 + b.x1) / 2.0) as f32, ((b.y1 + b.y0) / 2.0) as f32),
        Vec2::new((b.x1 - b.x0) as f32, (b.y1 - b.y0) as f32),
        css::BLUE,
    );
    fill(
        &mut gizmos,
        &[(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)],
        css::BLUE.with_alpha(0.01).into(),
    );
}

//! The edge of the world, from `border.ron`. An ECS resource, seeded from
//! content in `main`; the camera frames on it and never pans past.

use bevy::prelude::Resource;
use serde::Deserialize;

/// The edge of the world, `(x0, y0)` bottom-left to `(x1, y1)` top-right.
///
/// Still a section of `Content`/`ContentFile` for loading (every `*.ron` is the
/// same optional-everything file, so a mod overrides just this by shipping one),
/// then copied out as its own resource in `main` — the same shape `Calendar`
/// takes.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Resource)]
pub struct Border {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Border {
    /// `(x_min, x_max, y_min, y_max)` of the map edge, for the canvas bounds.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        (self.x0, self.x1, self.y0, self.y1)
    }
}

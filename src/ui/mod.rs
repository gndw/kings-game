//! The text overlays: the chronicle panel and the status bar. The camera and
//! map drawing live in `map`, the layout that spawns the panels in `startup`.

pub mod chronicle;
pub mod flag;
pub mod legend;
pub mod map;
pub mod startup;
pub mod status;

const FONT: f32 = 14.0;

/// Panel title colour.
const TITLE: bevy::color::Color = bevy::color::Color::srgb(0.75, 0.7, 0.45);

/// Gap between the stacked panels, so they read as separate boxes.
const GAP: f32 = 6.0;

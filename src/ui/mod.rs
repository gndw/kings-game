//! The text overlays: the chronicle panel, plus the resource bar along the top
//! and the status bar along the bottom. The camera lives in `camera`, the
//! gizmo map drawing in `map::components`, the layout that spawns the panels
//! in `startup`, and the root-layer input handlers (global keys + map
//! selection) in `input`.

use bevy::prelude::*;

pub mod camera;
pub mod character;
pub mod command_menu;
pub mod error;
pub mod event_popup;
pub mod input;
pub mod kingdom;
pub mod resource;
pub mod startup;
pub mod status;
pub mod wiki;

const FONT: f32 = 14.0;

/// Panel title colour.
const TITLE: bevy::color::Color = bevy::color::Color::srgb(0.75, 0.7, 0.45);

/// Gap between the stacked panels, so they read as separate boxes.
const GAP: f32 = 6.0;

/// Spawn a `TextSpan` with the panel font and the given colour. Every span
/// needs its own `TextFont`/`TextColor` (no inheritance from the parent) —
/// this helper centralises the trio.
pub(crate) fn spawn_span(p: &mut ChildSpawnerCommands, text: impl Into<String>, color: Color) {
    p.spawn((
        TextSpan::new(text),
        TextFont::from_font_size(FONT),
        TextColor(color),
    ));
}

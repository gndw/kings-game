//! The text overlays: the chronicle panel, plus the resource bar along the top
//! and the status bar along the bottom. The camera lives in `camera`, the
//! gizmo map drawing in `map::components`, the layout that spawns the panels
//! in `startup`, and the root-layer input handlers (global keys + map
//! selection) in `input`.

pub mod army;
pub mod buildings;
pub mod camera;
pub mod chronicle;
pub mod command_menu;
pub mod courts;
pub mod flag;
pub mod information;
pub mod input;
pub mod resource;
pub mod startup;
pub mod status;
pub mod wars;

const FONT: f32 = 14.0;

/// Panel title colour.
const TITLE: bevy::color::Color = bevy::color::Color::srgb(0.75, 0.7, 0.45);

/// Gap between the stacked panels, so they read as separate boxes.
const GAP: f32 = 6.0;

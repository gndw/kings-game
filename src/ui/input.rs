//! Root-layer input: global sim controls (quit, zoom, pause, speed) and map
//! selection (arrow keys).
//!
//! Every system here is gated to run only while
//! [`InputLayer::Root`](crate::resources::input_layer::InputLayer::Root) is
//! the active layer. When the command palette is up, the layer is
//! [`InputLayer::CommandMenu`](crate::resources::input_layer::InputLayer::CommandMenu)
//! and these systems skip — the palette owns every keystroke.

use crate::app::Game;
use crate::resources::calendar::Calendar;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

/// Run condition: the root input layer is active (palette is closed).
pub fn root_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Root
}

/// Global sim keys: quit (`Q`/`Esc`), zoom toggle (`Z`), pause toggle
/// (`Space`), and digit speed-jumps (`1`–`4`). Moved from
/// [`crate::app::input`] so input handling for the root layer lives in one
/// place; the run-if on the system list guarantees this only fires when the
/// root layer is active.
pub fn global_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<Game>,
    calendar: Res<Calendar>,
    mut fixed: ResMut<Time<Fixed>>,
    mut exit: MessageWriter<AppExit>,
) {
    // Escape closes the command palette while it's open, so it mustn't quit.
    if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        game.zoomed = !game.zoomed;
    }
    // Space toggles pause (multi-word search queries no longer collide with
    // this because the palette's `input_layer == CommandMenu` blocks the key
    // from reaching us).
    if keys.just_pressed(KeyCode::Space) {
        game.paused = !game.paused;
    }
    // Digits 1–4 jump straight to a speed and unpause, faster than stepping.
    // The index clamps if a mod lists fewer than four speeds.
    let last = calendar.speeds.len().saturating_sub(1);
    for (key, idx) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
    ] {
        if keys.just_pressed(key) {
            game.speed_idx = idx.min(last);
            game.paused = false;
        }
    }
    fixed.set_timestep_hz(f64::from(crate::app::speed(
        &calendar.speeds,
        game.speed_idx,
    )));
}

/// Arrow keys move the selection to the neighbouring land in that direction.
/// Exclusive: selection stepping reads many lands and writes the player's
/// selection, all through the one [`World`]. Moved out of `ui/map.rs` so
/// root-layer input has one home; the run-if keeps the arrows out of the
/// palette's way.
pub fn map_selection(world: &mut World) {
    let dir = [
        (KeyCode::ArrowLeft, (-1.0, 0.0)),
        (KeyCode::ArrowRight, (1.0, 0.0)),
        (KeyCode::ArrowUp, (0.0, 1.0)),
        (KeyCode::ArrowDown, (0.0, -1.0)),
    ]
    .into_iter()
    .find_map(|(k, d)| {
        world
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(k)
            .then_some(d)
    });
    let Some(dir) = dir else {
        return;
    };
    let sel = world.resource::<Game>().ctx.selected_land_id.clone();
    let Some(sel) = sel else {
        return;
    };
    if let Some(next) = crate::ctx::step(world, &sel, dir) {
        world.resource_mut::<Game>().ctx.selected_land_id = Some(next);
    }
}

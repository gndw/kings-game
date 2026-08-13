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

/// Global sim keys: quit (`Q`), open the palette (`C`), zoom toggle
/// (`Z`), pause toggle (`Space`), and digit speed-jumps (`1`–`4`). Moved
/// from [`crate::app::input`] so input handling for the root layer lives in
/// one place; the run-if on the system list guarantees this only fires when
/// the root layer is active.
///
/// Exclusive because opening the palette (`C`) calls
/// [`crate::ui::command_menu::open_command`], which spawns UI entities
/// through `&mut World`.
pub fn global_keys(world: &mut World) {
    let (toggle_palette, toggle_quit, toggle_zoom, toggle_pause, digit_pressed) = {
        let keys = world.resource::<ButtonInput<KeyCode>>();
        (
            keys.just_pressed(KeyCode::KeyC),
            keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Escape),
            keys.just_pressed(KeyCode::KeyZ),
            keys.just_pressed(KeyCode::Space),
            [
                (KeyCode::Digit1, 0usize),
                (KeyCode::Digit2, 1),
                (KeyCode::Digit3, 2),
                (KeyCode::Digit4, 3),
            ]
            .into_iter()
            .find_map(|(k, i)| keys.just_pressed(k).then_some(i)),
        )
    };

    if toggle_palette {
        crate::ui::command_menu::open_command(world);
    }
    // Escape closes the command palette while it's open, so it mustn't quit.
    if toggle_quit {
        world.write_message(AppExit::Success);
    }
    if toggle_zoom {
        world.resource_mut::<Game>().zoomed = !world.resource::<Game>().zoomed;
    }
    if toggle_pause {
        let mut game = world.resource_mut::<Game>();
        game.paused = !game.paused;
    }
    if let Some(idx) = digit_pressed {
        let last = world.resource::<Calendar>().speeds.len().saturating_sub(1);
        let mut game = world.resource_mut::<Game>();
        game.speed_idx = idx.min(last);
        game.paused = false;
    }
    let speed_idx = world.resource::<Game>().speed_idx;
    let speeds = world.resource::<Calendar>().speeds.clone();
    world
        .resource_mut::<Time<Fixed>>()
        .set_timestep_hz(f64::from(crate::app::speed(&speeds, speed_idx)));
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

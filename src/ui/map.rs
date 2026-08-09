//! The map's interaction layer. Visual drawing of the world-border, lands,
//! and per-kingdom castles all lives in the `map::components` siblings
//! (`border_graphic`, `land_graphic`, `holding_icon`); this module owns the
//! selection-step input only. The camera itself lives in `super::camera`;
//! the map geometry lives in the entity world (see `crate::ecs::Land`).

use crate::app::Game;
use bevy::prelude::*;

/// Arrow keys move the selection to the neighbouring land in that direction.
/// Exclusive: selection stepping reads many lands and writes the player's
/// selection, all through the one [`World`].
pub fn update_input(world: &mut World) {
    // The command palette owns the arrows while open; don't move the selection.
    if world.resource::<crate::ui::command_menu::CommandMenu>().open {
        return;
    }
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

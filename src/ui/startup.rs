//! The flex layout holding the two full-width bars and an empty middle row
//! where the camera draws the map.

use super::{character, kingdom, resource, status};
use bevy::prelude::*;

/// The two bars plus the right-docked overlays.
pub fn startup(mut commands: Commands) {
    // Layout: resource bar at top, an empty middle row that fills the
    // remaining vertical space (the map is gizmo-rendered on top of it via
    // the camera), status bar at the bottom. The kingdom + character panels
    // sit on top of the middle row as right-docked absolute overlays — they
    // hide each other via `set_visible`, never in the flex column.
    let panel = Color::srgba(0.1, 0.1, 0.1, 1.0);
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|root| {
            resource::spawn(root, panel);
            kingdom::spawn_shell(root);
            character::spawn_shell(root);
            root.spawn(Node {
                width: percent(100),
                flex_grow: 1.0,
                ..default()
            });
            status::spawn(root, panel);
        });
}

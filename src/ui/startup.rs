//! The flex layout holding the two full-width bars and an empty middle row
//! where the camera draws the map.

use super::{resource, status};
use bevy::prelude::*;

/// The two bars.
pub fn startup(mut commands: Commands) {
    // Layout: resource bar at top, an empty middle row that fills the
    // remaining vertical space (the map is gizmo-rendered on top of it via
    // the camera), status bar at the bottom. The right-side info panels
    // (information / courts / buildings / chronicle / wars / army) are
    // hidden for now — the `col` node that used to hold them is gone.
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
            root.spawn(Node {
                width: percent(100),
                flex_grow: 1.0,
                ..default()
            });
            status::spawn(root, panel);
        });
}

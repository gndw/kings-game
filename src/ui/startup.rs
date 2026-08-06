//! The flex layout holding the text panels and the two full-width bars.

use super::{GAP, actions, buildings, chronicle, information, resource, status};
use bevy::prelude::*;

pub(crate) const RIGHT_BAR: f32 = 0.3;

/// The two text panels.
pub fn startup(mut commands: Commands) {
    // The old terminal layout, as a flex tree: the resource bar on top, a row
    // holding the map and the right-hand column (information / buildings /
    // actions / chronicle), the status bar underneath.
    let panel = Color::srgba(0.1, 0.1, 0.1, 1.0);
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|root| {
            // Resource bar top, map row in the middle, status bar bottom.
            resource::spawn(root, panel);
            root.spawn(Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::End,
                ..default()
            })
            .with_children(|row| {
                // Legend on top taking what the chronicle leaves, chronicle
                // pinned to 30%, a gap between so they read as two panels.
                row.spawn(Node {
                    width: percent(RIGHT_BAR * 100.0),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(GAP),
                    ..default()
                })
                .with_children(|col| {
                    information::spawn(col, panel);
                    buildings::spawn(col, panel);
                    actions::spawn(col, panel);
                    chronicle::spawn(col, panel);
                });
            });
            status::spawn(root, panel);
        });
}

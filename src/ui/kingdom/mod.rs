//! The kingdom panel: a right-docked panel the player opens with **Enter** to
//! pin a kingdom. Stays pinned as the map selection moves; Enter on a
//! different kingdom switches the pinned kingdom, and Enter on the pinned
//! kingdom closes the panel.
//!
//! Rendered sections: kingdom name, land, ruler, courtiers, wars, armies,
//! buildings. Building row colors match the spec: red when the levy is raised,
//! yellow when the levy is below max, gold for profit, green for max levy,
//! gray for upkeep.

use bevy::prelude::*;

use super::{FONT, TITLE};

mod kingdom;
mod kingdom_army;
mod kingdom_buildings;
mod kingdom_courts;
mod kingdom_detail;
mod kingdom_war;

pub use kingdom::*;

/// Spawn the panel shell once, hidden, as a child of the root layout node.
/// Right-docked absolute overlay so the map keeps its area; width is a
/// fixed percent rather than a flex sibling so the panel doesn't compete
/// with the camera for layout space. Colours match the kingdom panel's
/// border/backdrop so the two panels read as one slot.
pub(super) fn spawn_shell(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            KingdomUIRoot,
            Node {
                position_type: PositionType::Absolute,
                right: px(0),
                top: px(0),
                bottom: px(0),
                width: percent(35),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(4),
                border: UiRect::all(px(1)),
                overflow: Overflow::clip(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.10, 0.12)),
            BorderColor::all(Color::srgba(0.6, 0.6, 0.65, 0.5)),
            GlobalZIndex(50),
        ))
        .with_children(|win| {
            win.spawn((
                Text::new("KINGDOM"),
                TextFont::from_font_size(FONT + 2.0),
                TextColor(TITLE),
            ));
            win.spawn((
                KingdomUIBody,
                Text::new(""),
                TextFont::from_font_size(FONT),
                TextColor(Color::WHITE),
            ));
        });
}

//! The character panel: a right-docked panel that *replaces* the kingdom
//! panel while the player drills into a character. Opened with **R** while
//! the kingdom panel is pinned (resolves to the kingdom's ruler); **Enter**
//! closes both panels, **Backspace** pops back to the still-pinned kingdom
//! panel.
//!
//! Rendered sections (one line each, matching the kingdom panel style):
//! `name house [gender] (age) [opinion]`, `ruler of: <kingdom>` (when the
//! character leads a kingdom), `gold`, `gold/m`, `levy`, and the six
//! skills. Opinion is suppressed when the character is the player.

use bevy::prelude::*;

use super::{FONT, TITLE};

mod character;
mod character_detail;
mod character_skills;
mod character_stats;

pub use character::*;

/// Spawn the panel shell once, hidden, as a child of the root layout node.
/// Same right-docked 35% width as the kingdom panel so they visually
/// replace each other and the camera shift in [`crate::ui::camera`] stays
/// in lock-step.
pub(super) fn spawn_shell(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            CharacterUIRoot,
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
                Text::new("CHARACTER"),
                TextFont::from_font_size(FONT + 2.0),
                TextColor(TITLE),
            ));
            win.spawn((
                CharacterUIBody,
                Text::new(""),
                TextFont::from_font_size(FONT),
                TextColor(Color::WHITE),
            ));
        });
}

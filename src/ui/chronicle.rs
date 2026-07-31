//! The chronicle panel on the right: the last few chronicle lines.

use super::{FONT, TITLE};
use crate::app::Game;
use bevy::prelude::*;

/// Lines of the chronicle kept on screen.
const CHRONICLE_LINES: usize = 30;

#[derive(Component)]
pub struct Chronicle;

/// The bottom 30% of the right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            height: percent(30),
            flex_direction: FlexDirection::Column,
            // 30 lines don't fit in 30% of the column; clip rather than spill.
            overflow: Overflow::clip(),
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("CHRONICLE"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((Chronicle, Text::new(""), TextFont::from_font_size(FONT)));
    });
}

pub fn update(game: Res<Game>, mut chronicle: Single<&mut Text, With<Chronicle>>) {
    let start = game.ctx.chronicles.len().saturating_sub(CHRONICLE_LINES);
    chronicle.0 = game.ctx.chronicles[start..].join("\n");
}

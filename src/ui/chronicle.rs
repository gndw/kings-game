//! The chronicle panel on the right: the last few chronicle lines.

use super::{FONT, TITLE};
use crate::resources::chronicle::Chronicles;
use bevy::prelude::*;

/// Lines of the chronicle kept on screen.
const CHRONICLE_LINES: usize = 10;

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
            // 10 lines fit comfortably in 30% of the column; clip rather than spill.
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

pub fn update(
    chronicles: Res<Chronicles>,
    mut chronicle: Single<&mut Text, With<Chronicle>>,
) {
    let start = chronicles.0.len().saturating_sub(CHRONICLE_LINES);
    chronicle.0 = chronicles.0[start..].join("\n");
}

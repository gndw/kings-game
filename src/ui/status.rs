//! The status bar along the bottom: run state, date, speed, keys.

use super::FONT;
use super::chronicle::Chronicle;
use crate::app::{Game, speed};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::color::palettes::css;
use bevy::prelude::*;

#[derive(Component)]
pub struct Status;

pub(super) fn spawn(root: &mut ChildSpawnerCommands, panel: Color) {
    root.spawn((
        Status,
        Text::new(""),
        TextFont::from_font_size(FONT),
        TextLayout::justify(Justify::Center),
        BackgroundColor(panel),
        Node {
            width: percent(100),
            justify_content: JustifyContent::Center,
            padding: UiRect::all(px(3)),
            ..default()
        },
    ))
    // The root text holds just the state word so it can be coloured on its
    // own; this span carries the rest of the bar.
    .with_child((Status, TextSpan::default(), TextFont::from_font_size(FONT)));
}

// ponytail: Bevy query types are just verbose, not complex
#[allow(clippy::type_complexity)]
pub fn update(
    game: Res<Game>,
    date: Res<Date>,
    calendar: Res<Calendar>,
    mut status: Single<(&mut Text, &mut TextColor), (With<Status>, Without<Chronicle>)>,
    mut status_rest: Single<&mut TextSpan, With<Status>>,
) {
    let (state, colour) = if game.paused {
        ("[PAUSED]", css::RED)
    } else {
        ("[RUNNING]", css::GREEN)
    };
    let (ref mut text, ref mut text_colour) = *status;
    text.0 = state.to_string();
    text_colour.0 = colour.into();
    // Gold and levy live in the resource bar along the top.
    status_rest.0 = format!("  {}  {} days/s  ·  C commands", *date, speed(&calendar.speeds, game.speed_idx));
}

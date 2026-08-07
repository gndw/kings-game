//! The INFORMATION panel in the right-hand column: the selected land's name
//! and the kingdom's ruler that holds it.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    CharacterDateOfBirth, CharacterName, CharacterOfHouse, HouseName, KingdomHold, KingdomLedBy,
    LandName, Registry,
};
use crate::game::age::age;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;

/// land / ruler detail block. Its `Text` is rewritten by [`update`] when the
/// selection changes.
#[derive(Component)]
pub struct LegendInfo;

/// The INFORMATION panel: title + the detail block. Spawned as a sibling
/// panel above `courts` in the right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("INFORMATION"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((LegendInfo, Text::new(""), TextFont::from_font_size(FONT)));
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    date: Res<Date>,
    calendar: Res<Calendar>,
    mut info: Single<&mut Text, With<LegendInfo>>,
    lands: Query<&LandName>,
    kingdoms: Query<(&KingdomHold, Option<&KingdomLedBy>)>,
    chars: Query<(&CharacterName, &CharacterDateOfBirth)>,
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
) {
    // Nothing selected, or a selected id the world doesn't resolve to a land:
    // blank the info text. The buildings panel clears itself independently.
    let Some((land_e, land_name)) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id))
        .and_then(|e| lands.get(e).ok().map(|n| (e, n)))
    else {
        info.0.clear();
        return;
    };

    // Section: land, ruler detail.
    let mut inf = format!("name:{}", land_name.0);
    if let Some((_, kingdom_led_by)) = kingdoms
        .iter()
        .find(|(kingdom_hold, _)| kingdom_hold.0 == land_e)
        && let Some(kingdom_led_by) = kingdom_led_by
        && let Ok((character_name, character_dob)) = chars.get(kingdom_led_by.0)
    {
        let house = character_of_house
            .get(kingdom_led_by.0)
            .ok()
            .and_then(|character_of_house| {
                houses.get(character_of_house.0).ok()
            })
            .map(|house_name| house_name.0.clone())
            .unwrap_or_default();
        let character_age = age(&character_dob.0, &date, &calendar);
        inf.push_str(&format!(
            "\nruler:{} of {} ({})",
            character_name.0, house, character_age
        ));
    }
    info.0 = inf;
}

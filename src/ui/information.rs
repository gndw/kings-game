//! The INFORMATION panel in the right-hand column: id, land, and the
//! kingdom (with ruler) that holds it.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    CharacterAge, CharacterName, CharacterOfHouse, HouseName, KingdomHold, KingdomLedBy,
    LandName, Registry, StringId,
};
use bevy::prelude::*;

/// id / land / kingdom detail block. Its `Text` is rewritten by [`update`]
/// when the selection changes.
#[derive(Component)]
pub struct LegendInfo;

/// The INFORMATION panel: title + the detail block. Spawned as a sibling
/// panel above `buildings` in the right-hand column.
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
    mut info: Single<&mut Text, With<LegendInfo>>,
    lands: Query<&LandName>,
    kingdoms: Query<(&StringId, &KingdomHold, Option<&KingdomLedBy>)>,
    chars: Query<(&CharacterName, &CharacterAge)>,
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
) {
    // Nothing selected, or a selected id the world doesn't resolve to a land:
    // blank the info text. The buildings panel clears itself independently.
    let Some((id, land_e, land_name)) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id).map(|e| (id.clone(), e)))
        .and_then(|(id, e)| lands.get(e).ok().map(|land_name| (id, e, land_name)))
    else {
        info.0.clear();
        return;
    };

    // Section: id, land, kingdom detail.
    let mut inf = format!("id:{id}\nname:{}", land_name.0);
    if let Some((kingdom_string_id, _, kingdom_led_by)) = kingdoms
        .iter()
        .find(|(_, kingdom_hold, _)| kingdom_hold.0 == land_e)
    {
        inf.push_str(&format!("\nkingdom:{} (seat)", kingdom_string_id.0));
        if let Some(kingdom_led_by) = kingdom_led_by
            && let Ok((character_name, character_age)) = chars.get(kingdom_led_by.0)
        {
            let house = character_of_house
                .get(kingdom_led_by.0)
                .ok()
                .and_then(|character_of_house| {
                    houses.get(character_of_house.0).ok()
                })
                .map(|house_name| house_name.0.clone())
                .unwrap_or_default();
            inf.push_str(&format!(
                "\nruler:{} of {} ({})",
                character_name.0, house, character_age.0
            ));
        }
    }
    info.0 = inf;
}

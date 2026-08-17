//! The INFORMATION panel in the right-hand column: the selected land's name
//! and the kingdom's ruler that holds it.

use super::{FONT, TITLE, spawn_span};
use crate::app::Game;
use crate::ecs::{
    CharacterDateOfBirth, CharacterGender, CharacterHasFather, CharacterHasHusband,
    CharacterHasMother, CharacterName, CharacterOfHouse, HouseName, KingdomHold, KingdomLedBy,
    KingdomName, LandName, Registry,
};
use crate::helper::age_helper::age;
use crate::helper::opinion_helper::{opinion_color, opinion_of};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;

/// land / ruler detail block. Its `TextSpan` children are rebuilt by [`update`].
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
        p.spawn((
            LegendInfo,
            Text::new(""),
            TextFont::from_font_size(FONT),
            TextColor(Color::WHITE),
        ));
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    date: Res<Date>,
    calendar: Res<Calendar>,
    info: Single<Entity, With<LegendInfo>>,
    mut commands: Commands,
    lands: Query<&LandName>,
    kingdoms: Query<(&KingdomHold, Option<&KingdomLedBy>, Option<&KingdomName>)>,
    chars: Query<(&CharacterName, &CharacterDateOfBirth, &CharacterGender)>,
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
    opinion_fathers: Query<&CharacterHasFather>,
    opinion_mothers: Query<&CharacterHasMother>,
    opinion_husbands: Query<&CharacterHasHusband>,
    opinion_memories: Query<(
        &crate::ecs::character::MemoryOfCharacter,
        &crate::ecs::character::MemoryTowardCharacter,
        &crate::ecs::character::MemoryUntilDate,
        &crate::ecs::character::MemoryKind,
    )>,
) {
    let info_e = *info;
    let player_e = game.ctx.player_character_id.as_deref().and_then(|id| registry.get(id));

    // Nothing selected, or a selected id the world doesn't resolve to a land:
    // blank the info text. The buildings panel clears itself independently.
    let Some((land_e, land_name)) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id))
        .and_then(|e| lands.get(e).ok().map(|n| (e, n)))
    else {
        commands.entity(info_e).despawn_children();
        return;
    };

    // Pull the kingdom (if any) holding this land: name + optional ruler.
    let kingdom_row = kingdoms
        .iter()
        .find(|(hold, _, _)| hold.0 == land_e);
    let kingdom_name = kingdom_row.and_then(|(_, _, n)| n.map(|n| n.0.clone()));
    let ruler = kingdom_row
        .and_then(|(_, led_by, _)| led_by.copied())
        .and_then(|kingdom_led_by| {
            let (character_name, character_dob, character_gender) =
                chars.get(kingdom_led_by.0).ok()?;
            let house = character_of_house
                .get(kingdom_led_by.0)
                .ok()
                .and_then(|cof| houses.get(cof.0).ok())
                .map(|hn| hn.0.clone())
                .unwrap_or_default();
            let character_age = age(&character_dob.0, &date, &calendar);
            let gender_marker = match character_gender {
                CharacterGender::Male => "m",
                CharacterGender::Female => "f",
            };
            Some((
                kingdom_led_by.0,
                character_name.0.clone(),
                house,
                character_age,
                gender_marker,
            ))
        });

    // Rebuild the line as `TextSpan` children so the opinion value can be
    // coloured independently of the surrounding text. Kingdom name sits on
    // top (when one exists), then the land, then the ruler.
    commands.entity(info_e).despawn_children();
    commands.entity(info_e).with_children(|p| {
        if let Some(kingdom_name) = kingdom_name {
            spawn_span(p, format!("{}\n", kingdom_name), TITLE);
        }
        spawn_span(p, format!("name:{}\n", land_name.0), Color::WHITE);
        if let Some((ruler_e, ruler_name, house, age, gender_marker)) = ruler {
            spawn_span(p, "ruler: ", Color::WHITE);
            spawn_span(p, format!("{} {}", ruler_name, house), Color::WHITE);
            spawn_span(p, format!(" [{}]", gender_marker), Color::WHITE);
            spawn_span(p, format!(" ({})", age), Color::WHITE);
            if let Some(player) = player_e.filter(|p| *p != ruler_e) {
                let op = opinion_of(
                    ruler_e,
                    player,
                    &character_of_house,
                    &opinion_fathers,
                    &opinion_mothers,
                    &opinion_husbands,
                    &opinion_memories, &*date,
                );
                spawn_span(p, " [", Color::WHITE);
                spawn_span(p, format!("{:+}", op), opinion_color(op));
                spawn_span(p, "]", Color::WHITE);
            }
        }
    });
}
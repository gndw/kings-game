//! Courtiers of the kingdom holding the selected land.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    CharacterName, CharacterOfHouse, CourtierOfCharacter, CourtierOfKingdom, CourtierType,
    HouseName, LandHeldBy, Registry,
};
use bevy::prelude::*;

#[derive(Component)]
pub struct LegendCourts;

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
            Text::new("COURT"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((LegendCourts, Text::new(""), TextFont::from_font_size(FONT)));
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    held_by: Query<&LandHeldBy>,
    courtiers: Query<(&CourtierOfKingdom, &CourtierOfCharacter, &CourtierType)>,
    characters: Query<(&CharacterName, &CharacterOfHouse)>,
    houses: Query<&HouseName>,
    mut text: Single<&mut Text, With<LegendCourts>>,
) {
    let kingdom = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|land| held_by.get(land).ok())
        .map(LandHeldBy::kingdom);
    let mut lines = courtiers
        .iter()
        .filter(|(k, _, _)| Some(k.0) == kingdom)
        .filter_map(|(_, c, role)| {
            let (name, house) = characters.get(c.0).ok()?;
            let house = houses.get(house.0).ok()?;
            Some(format!(
                "{} {} - {}",
                name.0,
                house.0,
                match role {
                    CourtierType::Courtier => "Courtier",
                }
            ))
        });
    text.0 = lines
        .next()
        .map(|first| {
            std::iter::once(first)
                .chain(lines)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "(none)".into());
}

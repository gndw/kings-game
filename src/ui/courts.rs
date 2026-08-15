//! Courtiers of the kingdom holding the selected land.

use super::{FONT, TITLE, spawn_span};
use crate::app::Game;
use crate::ecs::{
    CharacterHasFather, CharacterHasHusband, CharacterHasMother, CharacterName, CharacterOfHouse,
    CourtierOfCharacter, CourtierOfKingdom, CourtierType, HouseName, LandHeldBy, Registry,
};
use crate::helper::opinion_helper::{opinion_color, opinion_of};
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
        p.spawn((
            LegendCourts,
            Text::new(""),
            TextFont::from_font_size(FONT),
            TextColor(Color::WHITE),
        ));
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    courts: Single<Entity, With<LegendCourts>>,
    mut commands: Commands,
    held_by: Query<&LandHeldBy>,
    courtiers: Query<(&CourtierOfKingdom, &CourtierOfCharacter, &CourtierType)>,
    characters: Query<&CharacterName>,
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
    opinion_fathers: Query<&CharacterHasFather>,
    opinion_mothers: Query<&CharacterHasMother>,
    opinion_husbands: Query<&CharacterHasHusband>,
) {
    let courts_e = *courts;
    let kingdom = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|land| held_by.get(land).ok())
        .map(LandHeldBy::kingdom);
    let player_e = game.ctx.player_character_id.as_deref().and_then(|id| registry.get(id));

    let entries: Vec<(Entity, String, String, &'static str)> = courtiers
        .iter()
        .filter(|(k, _, _)| Some(k.0) == kingdom)
        .filter_map(|(_, c, role)| {
            let name = characters.get(c.0).ok()?.0.clone();
            let cof = character_of_house.get(c.0).ok()?;
            let house_name = houses.get(cof.0).ok()?.0.clone();
            let role_str = match role {
                CourtierType::Courtier => "Courtier",
            };
            Some((c.0, name, house_name, role_str))
        })
        .collect();

    commands.entity(courts_e).despawn_children();
    if entries.is_empty() {
        commands.entity(courts_e).with_children(|p| {
            spawn_span(p, "(none)", Color::WHITE);
        });
        return;
    }

    commands.entity(courts_e).with_children(|p| {
        for (i, (char_e, name, house, role)) in entries.into_iter().enumerate() {
            if i > 0 {
                spawn_span(p, "\n", Color::WHITE);
            }
            spawn_span(p, format!("{} {}", name, house), Color::WHITE);
            if let Some(player) = player_e {
                let op = opinion_of(
                    char_e,
                    player,
                    &character_of_house,
                    &opinion_fathers,
                    &opinion_mothers,
                    &opinion_husbands,
                );
                spawn_span(p, " [", Color::WHITE);
                spawn_span(p, format!("{:+}", op), opinion_color(op));
                spawn_span(p, "]", Color::WHITE);
            }
            spawn_span(p, format!(" - {}", role), Color::WHITE);
        }
    });
}

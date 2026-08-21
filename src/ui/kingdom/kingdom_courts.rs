//! Kingdom courtiers rows — character + house + age + (if applicable)
//! opinion.

use crate::ecs::character::{
    CharacterDateOfBirth, CharacterGender, CharacterName, CharacterOfHouse,
};
use crate::ecs::courtier::CourtierOfCharacter;
use crate::ecs::house::HouseName;
use crate::ecs::KingdomHasCourtiers;
use crate::helper::age_helper::age;
use crate::helper::opinion_helper::{opinion_color, opinion_of_via_world};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;

use super::super::TITLE;

pub(super) fn render_courtiers_spans(
    world: &mut World,
    kingdom_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let courtiers: Vec<Entity> = world
        .get::<KingdomHasCourtiers>(kingdom_e)
        .map(|k| k.iter().collect())
        .unwrap_or_default();
    if courtiers.is_empty() {
        return Vec::new();
    }
    let mut entries: Vec<(Entity, String, String, u32, &'static str)> = Vec::new();
    let mut court_chars = world.query::<&CourtierOfCharacter>();
    let mut characters =
        world.query::<(&CharacterName, &CharacterDateOfBirth, &CharacterGender)>();
    let mut character_of_house = world.query::<&CharacterOfHouse>();
    let mut houses = world.query::<&HouseName>();
    for courtier_e in courtiers {
        let Some(coc) = court_chars.get(world, courtier_e).ok() else {
            continue;
        };
        let char_e = coc.0;
        let Ok((name, dob, gender)) = characters.get(world, char_e) else {
            continue;
        };
        let house = character_of_house
            .get(world, char_e)
            .ok()
            .and_then(|cof| houses.get(world, cof.0).ok())
            .map(|hn| hn.0.clone())
            .unwrap_or_default();
        let char_age = age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
        let marker = match gender {
            CharacterGender::Male => "m",
            CharacterGender::Female => "f",
        };
        entries.push((char_e, name.0.clone(), house, char_age, marker));
    }
    if entries.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("courtiers:\n".to_string(), TITLE)];
    for (i, (char_e, name, house, age, marker)) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((format!("{} {}", name, house), Color::WHITE));
        spans.push((format!(" [{}] ({})", marker, age), Color::WHITE));
        if let Some(player) = player_e {
            let date = world.resource::<Date>().clone();
            let op = opinion_of_via_world(world, *char_e, player, &date);
            spans.push((" [".to_string(), Color::WHITE));
            spans.push((format!("{:+}", op), opinion_color(op)));
            spans.push(("]".to_string(), Color::WHITE));
        }
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

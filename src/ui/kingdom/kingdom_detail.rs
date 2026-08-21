//! Kingdom name, land, and ruler — the panel's header lines.

use crate::ecs::character::{
    CharacterDateOfBirth, CharacterGender, CharacterName, CharacterOfHouse,
};
use crate::ecs::house::HouseName;
use crate::ecs::LandName;
use crate::helper::age_helper::age;
use crate::helper::opinion_helper::{opinion_color, opinion_of_via_world};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;

use super::super::TITLE;

pub(super) fn render_name_spans(name: &str) -> Vec<(String, Color)> {
    vec![(format!("{}\n", name), TITLE)]
}

pub(super) fn render_land_spans(world: &World, land_e: Entity) -> Vec<(String, Color)> {
    match world.get::<LandName>(land_e) {
        Some(land_name) => vec![(format!("land: {}\n", land_name.0), Color::WHITE)],
        None => Vec::new(),
    }
}

pub(super) fn render_ruler_spans(
    world: &mut World,
    ruler_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let ent = world.entity(ruler_e);
    let Some(name) = ent.get::<CharacterName>() else {
        return Vec::new();
    };
    let Some(dob) = ent.get::<CharacterDateOfBirth>() else {
        return Vec::new();
    };
    let Some(gender) = ent.get::<CharacterGender>() else {
        return Vec::new();
    };
    let house = ent
        .get::<CharacterOfHouse>()
        .and_then(|cof| world.entity(cof.0).get::<HouseName>())
        .map(|hn| hn.0.clone())
        .unwrap_or_default();
    let ruler_age = age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
    let marker = match gender {
        CharacterGender::Male => "m",
        CharacterGender::Female => "f",
    };

    let mut spans = vec![
        ("ruler: ".to_string(), Color::WHITE),
        (format!("{} {}", name.0, house), Color::WHITE),
        (format!(" [{}] ({})", marker, ruler_age), Color::WHITE),
    ];
    if let Some(player) = player_e.filter(|p| *p != ruler_e) {
        let date = world.resource::<Date>().clone();
        let op = opinion_of_via_world(world, ruler_e, player, &date);
        spans.push((" [".to_string(), Color::WHITE));
        spans.push((format!("{:+}", op), opinion_color(op)));
        spans.push(("]\n".to_string(), Color::WHITE));
    } else {
        spans.push(("\n".to_string(), Color::WHITE));
    }
    spans
}

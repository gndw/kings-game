//! Kingdom name, land, ruler, and treasury — the panel's header lines.

use crate::ecs::character::{
    CharacterDateOfBirth, CharacterGender, CharacterName, CharacterOfHouse,
};
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::{KingdomGold, KingdomGoldYield, KingdomLevy};
use crate::ecs::LandName;
use crate::helper::age_helper::get_age;
use crate::helper::opinion_helper::{get_opinion_color, get_opinion_of};
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
    let ruler_age = get_age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
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
        let op = get_opinion_of(world, ruler_e, player, &date);
        spans.push((" [".to_string(), Color::WHITE));
        spans.push((format!("{:+}", op), get_opinion_color(op)));
        spans.push(("]\n".to_string(), Color::WHITE));
    } else {
        spans.push(("\n".to_string(), Color::WHITE));
    }
    // Realm treasury — this kingdom's gold/yield/levy (read off the ruler's
    // primary kingdom entity, which the panel already shows).
    if let Some(kingdom_e) = crate::helper::kingdom_helper::get_character_ruled_kingdoms(world, ruler_e)
        .first()
        .copied()
    {
        let gold = world.get::<KingdomGold>(kingdom_e).map(|g| g.0).unwrap_or(0);
        let yield_ = world.get::<KingdomGoldYield>(kingdom_e).map(|g| g.0).unwrap_or(0);
        let levy = world.get::<KingdomLevy>(kingdom_e).map(|l| l.0).unwrap_or(0);
        spans.push(("treasury: ".to_string(), Color::WHITE));
        spans.push((format!("{} gold", gold), Color::WHITE));
        spans.push((format!(" ({:+}/mo)", yield_), Color::WHITE));
        spans.push((format!("  {} levy", levy), Color::WHITE));
        spans.push(("\n".to_string(), Color::WHITE));
    }
    spans
}

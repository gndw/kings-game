//! Opinion derivation + display colour.
//!
//! [`get_opinion_of`] computes the score one character holds toward
//! another (+10 same house, +20 close family, +50 spouse, +memory
//! contribution). Range today 0..=∞; the return is `i32` so future negative
//! rules fit without a signature change.
//!
//! [`get_opinion_color`] maps a score to a display colour: -100 → red,
//! 0 → grey, +100 → green, linear piecewise between.

use crate::ecs::character::{
    CharacterHasFather, CharacterHasHusband, CharacterHasMother, CharacterOfHouse, MemoryKind,
    MemoryOfCharacter, MemoryTowardCharacter, MemoryUntilDate,
};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::Color;

/// Score `observer` holds toward `target` under the current rules (see module
/// doc). Uses `&mut World` so callers don't need to thread seven separate
/// queries through their system param list — Bevy panics past 16 params.
pub fn get_opinion_of(world: &mut World, observer: Entity, target: Entity, today: &Date) -> i32 {
    let mut v: i32 = 0;
    let o_house = world.get::<CharacterOfHouse>(observer).map(|c| c.0);
    let t_house = world.get::<CharacterOfHouse>(target).map(|c| c.0);
    if o_house.is_some() && o_house == t_house {
        v += 10;
    }
    let o_husband = world.get::<CharacterHasHusband>(observer).map(|c| c.0);
    let t_husband = world.get::<CharacterHasHusband>(target).map(|c| c.0);
    if o_husband == Some(target) || t_husband == Some(observer) {
        v += 50;
    }
    let fo = world.get::<CharacterHasFather>(observer).map(|c| c.0);
    let mo = world.get::<CharacterHasMother>(observer).map(|c| c.0);
    let ft = world.get::<CharacterHasFather>(target).map(|c| c.0);
    let mt = world.get::<CharacterHasMother>(target).map(|c| c.0);
    let parent_child = fo == Some(target)
        || mo == Some(target)
        || ft == Some(observer)
        || mt == Some(observer);
    let sibling = (fo.is_some() && fo == ft) || (mo.is_some() && mo == mt);
    if parent_child || sibling {
        v += 20;
    }
    let mut mem_q = world.query::<(
        &MemoryOfCharacter,
        &MemoryTowardCharacter,
        &MemoryUntilDate,
        &MemoryKind,
    )>();
    for (of, toward, until, kind) in mem_q.iter(world) {
        if of.0 != observer || toward.0 != target {
            continue;
        }
        if until.0 <= *today {
            continue;
        }
        match kind {
            MemoryKind::ReceivedGold { amount } => v += *amount as i32,
        }
    }
    v
}

/// Map an opinion value in [-100, 100] to a colour: grey at 0, green at +100,
/// red at -100, linear gradient between.
pub fn get_opinion_color(value: i32) -> Color {
    let v = (value.clamp(-100, 100)) as f32 / 100.0;
    if v >= 0.0 {
        // grey (0.5, 0.5, 0.5) → green (0.4, 0.85, 0.4)
        Color::srgb(0.5 + (-0.1) * v, 0.5 + 0.35 * v, 0.5 + (-0.1) * v)
    } else {
        let t = -v;
        // grey → red (0.85, 0.3, 0.3)
        Color::srgb(0.5 + 0.35 * t, 0.5 + (-0.2) * t, 0.5 + (-0.2) * t)
    }
}

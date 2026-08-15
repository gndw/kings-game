//! Opinion derivation + display colour.
//!
//! `opinion_of` computes the score one character holds toward another (+10
//! same house, +20 close family, +50 spouse, +memory contribution). Range today
//! 0..=∞; the return is `i32` so future negative rules fit without a signature
//! change.
//!
//! `opinion_color` maps a score to a display colour: -100 → red, 0 → grey,
//! +100 → green, linear piecewise between.

use crate::ecs::character::{
    CharacterHasFather, CharacterHasHusband, CharacterHasMother, CharacterOfHouse, MemoryKind,
    MemoryOfCharacter, MemoryTowardCharacter, MemoryUntilDate,
};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Query;
use bevy::prelude::Color;

/// opinion_of(observer, target, ...) — see module doc for the rules.
pub fn opinion_of(
    observer: Entity,
    target: Entity,
    houses: &Query<&CharacterOfHouse>,
    fathers: &Query<&CharacterHasFather>,
    mothers: &Query<&CharacterHasMother>,
    husbands: &Query<&CharacterHasHusband>,
    memories: &Query<(
        &MemoryOfCharacter,
        &MemoryTowardCharacter,
        &MemoryUntilDate,
        &MemoryKind,
    )>,
    today: &Date,
) -> i32 {
    let mut v: i32 = 0;
    if let (Ok(o), Ok(t)) = (houses.get(observer), houses.get(target))
        && o.0 == t.0
    {
        v += 10;
    }
    // Spouse: the wife carries `CharacterHasHusband`; the reverse on the
    // husband is a single Entity. Checking the husband side from either
    // observer or target catches both directions in one shot.
    if husbands.get(observer).map(|c| c.0).ok() == Some(target)
        || husbands.get(target).map(|c| c.0).ok() == Some(observer)
    {
        v += 50;
    }
    let fo = fathers.get(observer).map(|c| c.0).ok();
    let mo = mothers.get(observer).map(|c| c.0).ok();
    let ft = fathers.get(target).map(|c| c.0).ok();
    let mt = mothers.get(target).map(|c| c.0).ok();
    let parent_child = fo == Some(target) || mo == Some(target)
        || ft == Some(observer) || mt == Some(observer);
    let sibling = (fo.is_some() && fo == ft) || (mo.is_some() && mo == mt);
    if parent_child || sibling {
        v += 20;
    }
    // Memory contribution: sum every non-expired memory the observer carries
    // about deeds by the target. Expired memories are despawned by
    // `game::remembering::on_day`, so the query already skips them — the
    // until-date check is belt-and-braces in case a memory slips through.
    for (of, toward, until, kind) in memories.iter() {
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
pub fn opinion_color(value: i32) -> Color {
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

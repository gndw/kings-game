//! Aging and natural death.
//!
//! Per-character death is a scheduled event rather than a yearly chance roll —
//! each alive character carries a [`CharacterNextDeathEventDate`] and once the
//! sim's date reaches it, the system rolls once on [`SimRng`]. Surviving rolls
//! push the date forward, with the next horizon drawn from [`random_horizon_days`]
//! (older chars get shorter horizons, so they roll more often even with a fixed
//! per-roll probability). Dying flips [`CharacterIsAlive`] and fires
//! [`OnCharacterDied`].

use crate::app::Game;
use crate::ecs::{
    CharacterDateOfBirth, CharacterDateOfDeath, CharacterIsAlive, CharacterNextDeathEventDate,
};
use crate::events::OnCharacterDied;
use crate::helper::age_helper::age;
use crate::resources::{calendar::Calendar, date::Date};
use bevy::prelude::*;
use rand::TryRng;

/// Fixed 10% chance per roll; horizon-shrinkage alone ages characters out.
const DEATH_ROLL_PROBABILITY: u64 = u64::MAX / 10;

/// Per-day tick — see [`crate::schedules::OnDay`]. Cheap when nothing is due.
pub fn on_day(world: &mut World) {
    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();

    // Phase 1 — read pass over alive characters whose death-check date has
    // arrived. Decide what each one does, then release query borrows before
    // mutating entities or firing events.
    enum Action {
        Die,
        Reschedule(u32),
    }
    let mut actions: Vec<(Entity, Action)> = Vec::new();

    {
        let rng_arc = world.resource::<Game>().ctx.rng.clone();
        let mut rng = rng_arc.lock().unwrap();
        let mut q = world.query::<(
            Entity,
            &CharacterDateOfBirth,
            &CharacterIsAlive,
            &CharacterNextDeathEventDate,
        )>();
        for (e, dob, alive, next) in q.iter(world) {
            if !alive.0 || today < next.0 {
                continue;
            }
            let roll = rng.try_next_u64().unwrap_or(0);
            if roll < DEATH_ROLL_PROBABILITY {
                actions.push((e, Action::Die));
            } else {
                let current_age = age(&dob.0, &today, &calendar);
                let days = random_horizon_days(current_age, &mut rng);
                actions.push((e, Action::Reschedule(days)));
            }
        }
    }

    // Phase 2 — apply.
    for (e, action) in actions {
        match action {
            Action::Die => {
                world
                    .entity_mut(e)
                    .insert((CharacterIsAlive(false), CharacterDateOfDeath(Some(today))));
                world.trigger(OnCharacterDied { character: e, on_date: today });
            }
            Action::Reschedule(days) => {
                let new_date = today.after_days(days, &calendar);
                world.entity_mut(e).insert(CharacterNextDeathEventDate(new_date));
            }
        }
    }
}

/// Days until the next death-check roll, given the character's current age.
/// Older chars get shorter horizons so they roll more often even though the
/// per-roll chance stays fixed. The base is jittered by `0.5 + rng.next()`
/// so siblings in the same age band don't share a death date.
///
/// Bands are rough medieval-fantasy defaults — tune by editing the table.
pub fn random_horizon_days(age: u32, rng: &mut impl TryRng<Error = impl core::fmt::Debug>) -> u32 {
    let base = match age {
        a if a > 90 => 7,    // very old: a week
        a if a > 75 => 60,   // elderly: ~2 months
        a if a > 60 => 180,  // old: ~6 months
        a if a > 40 => 365,  // middle-aged: a year
        a if a > 15 => 730,  // adult: ~2 years
        a if a > 5 => 5_000, // child: ~14 years
        _ => 10_000,         // infant: ~28 years
    };
    let roll = rng.try_next_u64().unwrap_or(0);
    let normalized = (roll as f64) / (u64::MAX as f64);
    let jitter = 0.5 + normalized; // [0.5, 1.5)
    let jittered = (f64::from(base) * jitter).round() as u32;
    jittered.max(1)
}

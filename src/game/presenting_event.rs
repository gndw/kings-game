//! Event system: tick that decides *when* to present an event, the resolver
//! that runs the chosen effect, and the attendance picker.
//!
//! Trigger schedule: every [`crate::schedules::OnDay`]. Gated by an
//! `EventDeck` resource holding the next-due date and the pending event.
//!
//! Flow:
//! 1. `on_day` (exclusive system) — if today reaches `next_due_date` and no
//!    event is in flight, draw a weighted event id via `SimRng`, resolve an
//!    `attendee` entity, freeze the instance on `EventDeck::pending`, pause
//!    the game, and `world.trigger(OnEventPresented)` for the UI.
//! 2. `on_event_resolved` (observer) — takes the pending event, runs the
//!    chosen `ChoiceEffect`, clears the pending state, and schedules the
//!    next event 90–180 days out.
//! 3. `close_event` — Esc path. Forfeits the choice (no effect), clears the
//!    pending state, and reschedules.

use bevy::prelude::*;
use rand::TryRng;

use crate::app::Game;
use crate::commands::core::{alive_characters_excluding, transfer_with_gold_memory};
use crate::content::EventDeckState;
use crate::ecs::character::{CharacterLeads, CharacterLevy, CharacterOfHouse};
use crate::ecs::Registry;
use crate::events::{OnEventPresented, OnEventResolved};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;

use super::event_data::{ChoiceEffect, EVENT_DEFS, EventDef, EventInstance};

/// `EventDeck` resource: state for the trigger tick and the resolver.
#[derive(Resource, Default)]
pub struct EventDeck {
    /// Earliest day the tick may fire. State-supplied at startup;
    /// rewrites after every resolve or forfeit. Used by the trigger tick.
    pub next_due_date: Date,
    /// The in-flight event (`Some` while the popup is up). `None` between
    /// events; the tick fires only when this is `None`.
    pub pending: Option<EventInstance>,
    /// `false` until `on_day` runs the first time and seeds `next_due_date`.
    /// State-loaded games set this `true` (the state file provides a real
    /// `next_due_date`); year-0 sentinels leave it `false` so the random
    /// first-offset draw still fires for backwards compatibility with state
    /// files that predate the event system.
    pub initialized: bool,
}

/// Build the initial `EventDeck` from the state's `EventDeckState`. Year 0
/// (= `Date::default()`) is treated as "no state-supplied date", leaving
/// `initialized = false` and the first-run RNG fallback in place; any
/// other date marks the deck as initialised and skips the random first
/// offset. Called by `main.rs` after `mods::load`.
pub fn deck_from_state(state: &EventDeckState) -> EventDeck {
    let next_due_date = state.next_due_date;
    let initialized = next_due_date.year > 0;
    EventDeck {
        next_due_date,
        pending: None,
        initialized,
    }
}

/// Initial cooldown before the first event ever fires (days of in-game time).
/// 30–90 days ≈ 1–3 months in. Drawn from `SimRng` for replay determinism.
const FIRST_EVENT_OFFSET_MIN: u32 = 30;
const FIRST_EVENT_OFFSET_MAX: u32 = 90;
/// Cooldown between events the player resolves or forfeits. 90–180 days ≈
/// 3–6 in-game months.
const NEXT_EVENT_OFFSET_MIN: u32 = 90;
const NEXT_EVENT_OFFSET_MAX: u32 = 180;

/// Days of memory an event gold transfer grants. Matches the gift command
/// formula (`amount * 72`) so a 25-gold event memory lasts the same 5 years
/// as a 25-gold gift.
fn event_memory_days(amount: i64) -> u32 {
    (amount as u32).saturating_mul(72)
}

/// Tick. Runs on `OnDay`. If today ≥ `next_due_date` and no event is in
/// flight, draw an event, freeze the attendee, pause the game, and trigger
/// the presentation event. Exclusive because `world.trigger(...)` requires
/// it.
pub fn on_day(world: &mut World) {
    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();

    // First run: pick the initial offset; from then on `next_due_date` is
    // maintained by the resolver. Compute the RNG draw separately so the
    // mutable borrow on `EventDeck` doesn't overlap with the rng lock.
    let initialized = world.resource::<EventDeck>().initialized;
    if !initialized {
        let offset = rng_u32_in_range(world, FIRST_EVENT_OFFSET_MIN, FIRST_EVENT_OFFSET_MAX);
        let mut deck = world.resource_mut::<EventDeck>();
        deck.next_due_date = today.after_days(offset, &calendar);
        deck.initialized = true;
        return;
    }
    if world.resource::<EventDeck>().pending.is_some() {
        return;
    }
    if today < world.resource::<EventDeck>().next_due_date {
        return;
    }

    // Draw a weighted event id.
    let chosen_idx = {
        let total: u32 = EVENT_DEFS.iter().map(|d| d.weight).sum();
        let roll: u32 = rng_u32(world, total);
        let mut acc = 0u32;
        EVENT_DEFS
            .iter()
            .position(|d| {
                acc += d.weight;
                roll < acc
            })
            .unwrap_or(EVENT_DEFS.len() - 1)
    };
    let def = &EVENT_DEFS[chosen_idx];

    // Pick an attendee; if the named kind can't be filled (no matching
    // characters in the current world), reschedule +1 day and bail.
    let candidates = pick_attendee_candidates(world, def);
    let needs_attendee = def.choices.iter().any(|c| !matches!(c.effect, ChoiceEffect::None));
    if candidates.is_empty() && needs_attendee {
        let mut deck = world.resource_mut::<EventDeck>();
        deck.next_due_date = today.after_days(1, &calendar);
        return;
    }
    let attendee = if candidates.is_empty() {
        None
    } else {
        Some(pick_one(world, &candidates))
    };

    world.resource_mut::<EventDeck>().pending = Some(EventInstance {
        def_index: chosen_idx,
        attendee,
    });
    world.resource_mut::<Game>().paused = true;
    world.trigger(OnEventPresented);
}

/// Resolves the chosen effect (or skips for `None`), clears the pending
/// state, and schedules the next event. Sole `OnEventResolved` observer for
/// game-logic side effects; the UI hides the popup in its own observer.
///
/// Observer callbacks can't be exclusive (`&mut World`) in Bevy 0.19 — they
/// take `Commands` and queue the closure for the next exclusive opportunity,
/// matching the pattern in `ui::error::on_error_occurred` and
/// `ui::event_popup::on_event_resolved`. `Commands` is fine even when we
/// don't actually need structural changes (entity spawn/despawn) — only the
/// `world.trigger` re-entry through the closure requires `&mut World`.
pub fn on_event_resolved(trigger: On<OnEventResolved>, mut commands: Commands) {
    let choice = trigger.event().choice;
    commands.queue(move |world: &mut World| {
        resolve_choice(world, choice);
    });
}

fn resolve_choice(world: &mut World, choice: Option<usize>) {
    let (def_index, attendee_e) = {
        let mut deck = world.resource_mut::<EventDeck>();
        let Some(pending) = deck.pending.take() else {
            return;
        };
        (pending.def_index, pending.attendee)
    };
    let def = &EVENT_DEFS[def_index];

    if let Some(idx) = choice
        && let Some(c) = def.choices.get(idx)
    {
        run_effect(world, c.effect, attendee_e);
    }

    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();
    schedule_next(world, today, &calendar);
    world.resource_mut::<Game>().paused = false;
}

fn run_effect(world: &mut World, effect: ChoiceEffect, attendee: Option<Entity>) {
    let player_e = match player_entity(world) {
        Some(p) => p,
        None => return,
    };
    match effect {
        ChoiceEffect::None => {}
        ChoiceEffect::GiveGold { amount } => {
            let Some(attendee_e) = attendee else { return };
            let until = today_after_days(world, event_memory_days(amount));
            transfer_with_gold_memory(world, player_e, attendee_e, amount, until);
        }
        ChoiceEffect::ReceiveGold { amount } => {
            let Some(attendee_e) = attendee else { return };
            let until = today_after_days(world, event_memory_days(amount));
            transfer_with_gold_memory(world, attendee_e, player_e, amount, until);
        }
    }
}

fn schedule_next(world: &mut World, today: Date, calendar: &Calendar) {
    let offset: u32 = rng_u32_in_range(
        world,
        NEXT_EVENT_OFFSET_MIN,
        NEXT_EVENT_OFFSET_MAX,
    );
    let mut deck = world.resource_mut::<EventDeck>();
    deck.next_due_date = today.after_days(offset, calendar);
}

fn today_after_days(world: &World, days: u32) -> Date {
    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();
    today.after_days(days, &calendar)
}

/// Build the candidate list for `def`. Returns the empty `Vec` when no
/// character in the current world fits. Caller decides what `Option<Entity>`
/// to use as the resolved attendee — see `on_day`.
fn pick_attendee_candidates(world: &World, def: &EventDef) -> Vec<Entity> {
    let Some(actor) = player_entity(world) else {
        return Vec::new();
    };
    match def.id {
        "event:wayfaring_stranger" => pick_minor_other_house(world, actor),
        "event:envoy_house" => pick_house_leader_other_house(world, actor),
        "event:foreign_knight" => pick_levy_bearer_other_house(world, actor),
        _ => Vec::new(),
    }
}

fn pick_minor_other_house(world: &World, actor: Entity) -> Vec<Entity> {
    let actor_house = world.get::<CharacterOfHouse>(actor).map(|c| c.0);
    let chars = alive_characters_excluding(world, actor);
    let minor_other: Vec<Entity> = chars
        .into_iter()
        .filter(|(_, e)| {
            let levy = world.get::<CharacterLevy>(*e).map(|l| l.0).unwrap_or(0);
            let other_house = world
                .get::<CharacterOfHouse>(*e)
                .map(|c| actor_house != Some(c.0))
                .unwrap_or(true);
            levy == 0 && other_house
        })
        .map(|(_, e)| e)
        .collect();
    if !minor_other.is_empty() {
        return minor_other;
    }
    alive_characters_excluding(world, actor)
        .into_iter()
        .map(|(_, e)| e)
        .collect()
}

fn pick_house_leader_other_house(world: &World, actor: Entity) -> Vec<Entity> {
    let actor_house = world.get::<CharacterOfHouse>(actor).map(|c| c.0);
    alive_characters_excluding(world, actor)
        .into_iter()
        .filter(|(_, e)| {
            let leads = world
                .get::<CharacterLeads>(*e)
                .map(|cl| !cl.kingdoms().is_empty())
                .unwrap_or(false);
            let other_house = world
                .get::<CharacterOfHouse>(*e)
                .map(|c| actor_house != Some(c.0))
                .unwrap_or(true);
            leads && other_house
        })
        .map(|(_, e)| e)
        .collect()
}

fn pick_levy_bearer_other_house(world: &World, actor: Entity) -> Vec<Entity> {
    let actor_house = world.get::<CharacterOfHouse>(actor).map(|c| c.0);
    alive_characters_excluding(world, actor)
        .into_iter()
        .filter(|(_, e)| {
            let levy = world.get::<CharacterLevy>(*e).map(|l| l.0).unwrap_or(0);
            let other_house = world
                .get::<CharacterOfHouse>(*e)
                .map(|c| actor_house != Some(c.0))
                .unwrap_or(true);
            levy > 0 && other_house
        })
        .map(|(_, e)| e)
        .collect()
}

fn pick_one(world: &mut World, candidates: &[Entity]) -> Entity {
    let idx: usize = rng_usize(world, candidates.len());
    candidates[idx]
}

fn player_entity(world: &World) -> Option<Entity> {
    world
        .resource::<Game>()
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| world.resource::<Registry>().get(id))
}

fn rng_u32(world: &mut World, n: u32) -> u32 {
    let mut rng = world.resource::<Game>().ctx.rng.lock().unwrap();
    rng.try_next_u32().unwrap() % n
}

fn rng_u32_in_range(world: &mut World, min: u32, max: u32) -> u32 {
    let span = max - min + 1;
    min + rng_u32(world, span)
}

fn rng_usize(world: &mut World, n: usize) -> usize {
    let mut rng = world.resource::<Game>().ctx.rng.lock().unwrap();
    rng.try_next_u64().unwrap() as usize % n
}

// ponytail: at the moment the resolver is one-shot (no in-flight save), no
// queueing, and no conditional effects. If events grow to "this choice
// branches into one of three sub-events", lift the matching out of
// `run_effect` into a per-event handler — three hand-written arms beat a
// RON-effect vocabulary until that vocab has proven value.

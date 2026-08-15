//! Event system: tick that decides *when* to present an event, and the
//! resolver that calls the script's `effect(world)` for the chosen choice.
//!
//! All event authoring lives in `event-<id>.rhai` files. This module only
//! handles trigger timing, character resolution, and plumbing the chosen
//! choice into the script call.
//!
//! Trigger schedule: every [`crate::schedules::OnDay`]. Gated by an
//! `EventDeck` resource holding the next-due date and the pending event.
//!
//! Flow:
//! 1. `on_day` (exclusive system) — if today reaches `next_due_date` and no
//!    event is in flight, call each event's `can_trigger(world)` to filter
//!    the candidate pool, draw a weighted event id via `SimRng`, call the
//!    chosen event's `characters(world)` to resolve the event's characters
//!    (the script does its own RNG pick via `world.ctx.rng`), freeze the
//!    instance on `EventDeck::pending`, pause the game, and
//!    `world.trigger(OnEventPresented)` for the UI.
//! 2. `on_event_resolved` (observer) — calls the chosen event's
//!    `effect(world)`, clears the pending state, and schedules the next
//!    event 90–180 days out.
//! 3. `close_event` — Esc path. Forfeits the choice (no effect), clears the
//!    pending state, and reschedules.

use bevy::prelude::*;
use rand::TryRng;
use rhai::Scope;
use std::sync::Arc;

use crate::app::Game;
use crate::content::EventDeckState;
use crate::ecs::Registry;
use crate::observers::{OnEventPresented, OnEventResolved};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::resources::event_scripts::EventScripts;
use crate::script_ctx::{ScriptCtx, attach_ctx, build_world_view};

/// `EventDeck` resource: state for the trigger tick and the resolver.
#[derive(Resource, Default)]
pub struct EventDeck {
    /// Earliest day the tick may fire. State-supplied at startup;
    /// rewrites after every resolve or forfeit. Used by the trigger tick.
    pub next_due_date: Date,
    /// The in-flight event (`Some` while the popup is up). `None` between
    /// events; the tick fires only when this is `None`.
    pub pending: Option<EventInstance>,
}

/// The running state of one in-flight event. Stored on `EventDeck::pending`;
/// the popup renders from it. `characters` is the slice the event script
/// picked (via its own RNG); empty for ambient events.
pub struct EventInstance {
    /// Index into `EventScripts.events` — the chosen event's compiled script.
    pub def_index: usize,
    /// The characters this event is about, in order. The script picked them
    /// via `world.ctx.rng` inside `characters(world)`. Empty for events
    /// with no characters (ambient / global announcements).
    pub characters: Vec<Entity>,
}

/// Build the initial `EventDeck` from the state's `EventDeckState`. Called by
/// `main.rs` after `mods::load`.
pub fn deck_from_state(state: &EventDeckState) -> EventDeck {
    let next_due_date = state.next_due_date;
    EventDeck {
        next_due_date,
        pending: None,
    }
}

/// Cooldown between events the player resolves or forfeits. 90–180 days ≈
/// 3–6 in-game months.
const NEXT_EVENT_OFFSET_MIN: u32 = 90;
const NEXT_EVENT_OFFSET_MAX: u32 = 180;

/// Tick. Runs on `OnDay`. If today ≥ `next_due_date` and no event is in
/// flight, filter events by `can_trigger`, draw one weighted, resolve its
/// characters via the script's `characters` (which picks them itself),
/// freeze the instance on `EventDeck::pending`, pause the game, and
/// trigger the presentation event. Exclusive because `world.trigger(...)`
/// requires it.
pub fn on_day(world: &mut World) {
    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();

    if world.resource::<EventDeck>().pending.is_some() {
        return;
    }
    if today < world.resource::<EventDeck>().next_due_date {
        return;
    }

    let Some(player_e) = player_entity(world) else {
        // No player → can't run anything. Reschedule +1 day and bail.
        let mut deck = world.resource_mut::<EventDeck>();
        deck.next_due_date = today.after_days(1, &calendar);
        return;
    };

    // Build the world snapshot once for this tick — same data for every
    // event's can_trigger / characters call. Characters is `[]` and
    // choice_idx is `0` (modders ignore these fields outside `effect`).
    let base_world = build_world_view(world, player_e, &[], 0);

    // Filter: keep only events that pass can_trigger. Snapshot the result
    // (indices + weights) before dropping the resource borrow so the RNG
    // draw doesn't collide.
    let (eligible, weights): (Vec<usize>, Vec<u32>) = {
        let scripts = world.resource::<EventScripts>();
        let engine = &scripts.engine;
        let mut eligible = Vec::new();
        let mut weights = Vec::new();
        for (i, ev) in scripts.events.iter().enumerate() {
            if ev.call_can_trigger(engine, base_world.clone()) {
                let w = ev.call_weight(engine).unwrap_or(0);
                eligible.push(i);
                weights.push(w);
            }
        }
        (eligible, weights)
    };

    if eligible.is_empty() || weights.iter().sum::<u32>() == 0 {
        let mut deck = world.resource_mut::<EventDeck>();
        deck.next_due_date = today.after_days(1, &calendar);
        return;
    }

    let total: u32 = weights.iter().sum();
    let roll: u32 = rng_u32(world, total);
    let mut acc = 0u32;
    let chosen_idx = weights
        .iter()
        .position(|w| {
            acc += w;
            roll < acc
        })
        .map(|p| eligible[p])
        .unwrap_or(*eligible.last().unwrap());

    // Resolve the characters via the script. The script is responsible for
    // any RNG pick (via `world.ctx.rng`); the runtime uses the returned
    // entities as-is. Empty array → ambient (no characters).
    let characters: Vec<Entity> = {
        let scripts = world.resource::<EventScripts>();
        let engine = &scripts.engine;
        let ev = &scripts.events[chosen_idx];
        ev.call_characters(engine, base_world.clone())
    };

    world.resource_mut::<EventDeck>().pending = Some(EventInstance {
        def_index: chosen_idx,
        characters,
    });
    world.resource_mut::<Game>().paused = true;
    world.trigger(OnEventPresented);
}

/// Resolves the chosen effect (or skips if no choice was made), clears the
/// pending state, and schedules the next event. Sole `OnEventResolved`
/// observer for game-logic side effects; the UI hides the popup in its own
/// observer.
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
    let (def_index, characters) = {
        let mut deck = world.resource_mut::<EventDeck>();
        let Some(pending) = deck.pending.take() else {
            return;
        };
        (pending.def_index, pending.characters)
    };

    let Some(player_e) = player_entity(world) else {
        // No player — skip the effect, schedule next.
        let today = *world.resource::<Date>();
        let calendar = world.resource::<Calendar>().clone();
        schedule_next(world, today, &calendar);
        return;
    };

    // Call the event script's `effect(world)`. Pass a `ScriptCtx` so the
    // script can mutate the world via the registered API.
    //
    // The engine lives inside the `EventScripts` resource; we extract a raw
    // pointer to it (the resource outlives this function), then drop the
    // resource borrow before constructing `ScriptCtx` (which needs `&mut
    // World` and would otherwise conflict).
    let choice_idx = choice.unwrap_or(0);
    let (engine_ptr, ast): (*const rhai::Engine, Option<Arc<rhai::AST>>) = {
        let scripts = world.resource::<EventScripts>();
        match scripts.events.get(def_index) {
            Some(ev) => (&scripts.engine as *const rhai::Engine, Some(ev.ast.clone())),
            None => (std::ptr::null(), None),
        }
    };
    let Some(ast) = ast else {
        let today = *world.resource::<Date>();
        let calendar = world.resource::<Calendar>().clone();
        schedule_next(world, today, &calendar);
        return;
    };
    if !engine_ptr.is_null() {
        // SAFETY: the `EventScripts` resource lives in `world`, which we
        // hold exclusively for the duration of this function. No other code
        // path can remove or replace the resource while we hold `&mut World`.
        let engine: &rhai::Engine = unsafe { &*engine_ptr };
        let mut scope = Scope::new();
        let map = build_world_view(world, player_e, &characters, choice_idx);
        let map = attach_ctx(map, ScriptCtx::new(world));
        let _ = engine.call_fn::<()>(&mut scope, &ast, "effect", (map,));
    }

    let today = *world.resource::<Date>();
    let calendar = world.resource::<Calendar>().clone();
    schedule_next(world, today, &calendar);
    world.resource_mut::<Game>().paused = false;
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

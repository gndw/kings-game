//! What scripts asked for, and the only place it lands on the world.

use super::script_ctx::ScriptCtx;
use crate::ctx::Ctx;
use rhai::{Engine, ImmutableString};
use std::sync::Mutex;

/// Something a script asked the simulation to do. Collected while the hooks
/// run and applied afterwards, in order, so a script never holds a borrow on
/// `Ctx`. The `String` is the character the effect lands on.
pub(super) enum Effect {
    AddChronicle(String),
    AddCharacterGold(String, i64),
    SetCharacterLevy(String, u64),
    SetCharacterGoldYield(String, i64),
}

/// The writing half of the script surface. Called by [`super::register`], which
/// registers everything a script may read.
pub(super) fn register(engine: &mut Engine) {
    engine
        .register_fn(
            "add_character_gold",
            |c: &mut ScriptCtx, id: ImmutableString, n: i64| {
                c.push(Effect::AddCharacterGold(id.to_string(), n))
            },
        )
        // Negative levy is meaningless, so it floors at zero rather than
        // wrapping the `u64` on the way in. A negative gold yield is not —
        // that's a realm whose garrisons cost more than its holdings earn.
        .register_fn(
            "set_character_levy",
            |c: &mut ScriptCtx, id: ImmutableString, n: i64| {
                c.push(Effect::SetCharacterLevy(id.to_string(), n.max(0) as u64))
            },
        )
        .register_fn(
            "set_character_gold_yield",
            |c: &mut ScriptCtx, id: ImmutableString, n: i64| {
                c.push(Effect::SetCharacterGoldYield(id.to_string(), n))
            },
        )
        .register_fn(
            "add_chronicle",
            |c: &mut ScriptCtx, line: ImmutableString| {
                c.push(Effect::AddChronicle(line.to_string()))
            },
        );
}

/// Empty the queue into `ctx`, in the order the scripts filled it.
///
/// An effect naming a character the map doesn't have is dropped, not an error:
/// a mod may legitimately be written against a bigger roster.
pub(super) fn drain(out: &Mutex<Vec<Effect>>, ctx: &mut Ctx) {
    for effect in out.lock().unwrap().drain(..) {
        match effect {
            Effect::AddChronicle(line) => ctx.chronicles.push(line),
            Effect::AddCharacterGold(id, n) => {
                if let Some(c) = ctx.state.character_mut(&id) {
                    c.gold = c.gold.saturating_add(n);
                }
            }
            Effect::SetCharacterLevy(id, n) => {
                if let Some(c) = ctx.state.character_mut(&id) {
                    c.levy = n;
                }
            }
            Effect::SetCharacterGoldYield(id, n) => {
                if let Some(c) = ctx.state.character_mut(&id) {
                    c.gold_yield = n;
                }
            }
        }
    }
}

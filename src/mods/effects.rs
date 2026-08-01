//! What scripts asked for, and the only place it lands on the world.

use crate::ctx::Ctx;
use std::sync::Mutex;

/// Something a script asked the simulation to do. Collected while the hooks
/// run and applied afterwards, in order, so a script never holds a borrow on
/// `Ctx`. The `String` is the character the effect lands on.
pub(super) enum Effect {
    Chronicle(String),
    AddGold(String, i64),
    SetLevy(String, u64),
}

/// Empty the queue into `ctx`, in the order the scripts filled it.
///
/// An effect naming a character the map doesn't have is dropped, not an error:
/// a mod may legitimately be written against a bigger roster.
pub(super) fn drain(out: &Mutex<Vec<Effect>>, ctx: &mut Ctx) {
    for effect in out.lock().unwrap().drain(..) {
        match effect {
            Effect::Chronicle(line) => ctx.chronicles.push(line),
            Effect::AddGold(id, n) => {
                if let Some(c) = ctx.content.character_mut(&id) {
                    c.gold = c.gold.saturating_add(n);
                }
            }
            Effect::SetLevy(id, n) => {
                if let Some(c) = ctx.content.character_mut(&id) {
                    c.levy = n;
                }
            }
        }
    }
}

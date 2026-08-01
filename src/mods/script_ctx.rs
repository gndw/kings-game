//! The value every hook is handed. What it exposes to Rhai is [`super::register`].

use super::effects::Effect;
use super::view::RealmView;
use crate::ctx::Ctx;
use crate::rng::SimRng;
use rand::RngExt;
use std::sync::{Arc, Mutex};

/// What a script sees: a copy of the day's read-only state, plus handles to the
/// seeded RNG and the effect list.
///
/// Copied rather than borrowed because Rhai values must be `'static`. The
/// readable fields are a snapshot taken before any hook ran this tick — they do
/// not move as effects accumulate.
///
/// Fields and methods are `pub(super)` because `register` builds the script
/// surface out of them; outside `mods` this type is opaque.
#[derive(Clone)]
pub struct ScriptCtx {
    pub(super) year: i64,
    pub(super) month: i64,
    pub(super) day: i64,
    pub(super) tick: i64,
    /// The currently selected land's id, or "" if nothing is selected.
    pub(super) land: String,
    /// The character the player is playing as, for scripts that want to treat
    /// them differently — chronicle their taxes and no one else's, say.
    pub(super) player: String,
    pub(super) realms: Arc<RealmView>,
    pub(super) rng: Arc<Mutex<SimRng>>,
    pub(super) out: Arc<Mutex<Vec<Effect>>>,
}

impl ScriptCtx {
    /// This tick's snapshot. Built once per tick and cloned per mod per hook —
    /// the realms sit behind an `Arc`, so that's a refcount bump.
    pub(super) fn build(ctx: &Ctx, out: Arc<Mutex<Vec<Effect>>>) -> Self {
        ScriptCtx {
            year: i64::from(ctx.date.year),
            month: i64::from(ctx.date.month),
            day: i64::from(ctx.date.day),
            tick: ctx.tick_count as i64,
            land: ctx.selected_region.clone().unwrap_or_default(),
            player: ctx.player_character_id.clone(),
            realms: Arc::new(RealmView::build(&ctx.content)),
            rng: ctx.rng.clone(),
            out,
        }
    }

    /// Uniform in `[0, 1)`, drawn from the game's seeded RNG so that a mod
    /// using randomness still replays exactly from its seed.
    pub(super) fn rand(&mut self) -> f64 {
        self.rng.lock().unwrap().random::<f64>()
    }

    /// Queue an effect for after the hooks have run. See [`super::effects`].
    pub(super) fn push(&mut self, effect: Effect) {
        self.out.lock().unwrap().push(effect);
    }
}

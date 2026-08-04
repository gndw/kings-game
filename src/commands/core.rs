//! The player-command abstraction + the registry the palette drives, plus the
//! shared helpers every command reaches for (a fresh id, a chronicle line, the
//! "lands this actor rules" walk).
//!
//! A [`Command`] is self-describing: it owns its rules (validation), its UI (a
//! fixed run of selection [`steps`](Command::step_items)), and its effect
//! ([`execute`](Command::execute)). The command palette
//! ([`crate::ui::command_menu`]) drives the steps generically — it knows nothing
//! about any concrete command — so adding a command is a new struct + a line in
//! [`CommandRegistry::default`], not edits to the palette.

use std::sync::Arc;

use super::construct_building::ConstructBuilding;
use super::destroy_building::DestroyBuilding;
use crate::app::Game;
use crate::ecs::{HeldBy, Holds, LandName, Leads, Registry, StringId};
use crate::resources::chronicle::Chronicles;
use bevy::ecs::world::World;
use bevy::prelude::{RelationshipTarget, Resource};
use rand::TryRng;

/// One selectable row in a command's step list. `label` is what the player
/// sees; `value` is the id the command reads back in a later step or in
/// [`execute`](Command::execute).
pub struct MenuItem {
    pub label: String,
    pub value: String,
}

/// A choice the player made at an earlier step: the [`MenuItem`] they picked.
#[derive(Clone)]
pub struct Choice {
    pub label: String,
    pub value: String,
}

/// A player command: *what* to do, self-describing its own UI.
///
/// A command is a fixed run of selection steps (step `0` → … →
/// `step_count() - 1`); the last step's pick runs
/// [`execute`](Command::execute). Each step's items come from the world and the
/// choices made so far, so a later step can depend on an earlier one (e.g. the
/// buildings standing on the land picked at step 0).
///
/// The actor (a character id) is *who* the command runs for — the player today,
/// an AI/replay peer later — and is passed to both [`step_items`] and
/// [`execute`] so a command never assumes it is the player.
///
/// [`step_items`]: Command::step_items
pub trait Command: Send + Sync {
    /// Display name on the top-level command list.
    fn name(&self) -> &str;

    /// Window title while `step` (0-indexed) is showing.
    fn step_title(&self, step: usize) -> &str;

    /// How many selection steps before [`execute`](Command::execute) runs.
    fn step_count(&self) -> usize;

    /// The selectable items for `step`, given the choices made at earlier steps
    /// and a read-only world snapshot.
    fn step_items(
        &self,
        step: usize,
        choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem>;

    /// Run the effect. `choices` has one entry per step. Validate here too — a
    /// chronicle line on rejection is the convention (see construct/destroy).
    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World);
}

/// The roster of commands the palette offers. Held as a resource so the palette
/// is driven by *what's registered*, not a hardcoded list — adding a command is
/// a new [`Command`] struct + a [`register`](Self::register) line, no palette
/// edits, and a mod/plugin could push its own before `App::run`.
///
/// `Arc<dyn Command>` so the palette can hand a command to `execute` (which
/// needs `&mut World`) without holding the registry's borrow.
#[derive(Resource)]
pub struct CommandRegistry {
    pub commands: Vec<Arc<dyn Command>>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        let mut r = CommandRegistry { commands: Vec::new() };
        r.register(Arc::new(ConstructBuilding));
        r.register(Arc::new(DestroyBuilding));
        r
    }
}

impl CommandRegistry {
    /// Register `cmd` at the end of the list.
    pub fn register(&mut self, cmd: Arc<dyn Command>) {
        self.commands.push(cmd);
    }
}

/// The lands `actor` rules (can act on): actor → [`Leads`] → kingdom → its
/// [`Holds`] collection (the auto-maintained reverse of each land's `HeldBy`).
/// Walks the relationship targets with `world::get` so it stays a `&World` read
/// (`world::query` needs `&mut World`); the same target `ui::legend` iterates.
pub(super) fn ruled_lands(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(kingdom_e) = world.get::<Leads>(actor_e).map(|l| l.kingdom()) else {
        return Vec::new();
    };
    let Some(holds) = world.get::<Holds>(kingdom_e) else {
        return Vec::new();
    };
    holds
        .iter()
        .filter_map(|land_e| {
            let sid = world.get::<StringId>(land_e)?;
            let name = world.get::<LandName>(land_e)?;
            Some((sid.0.clone(), name.0.clone()))
        })
        .collect()
}

/// True if `actor` rules `land_id` (their [`Leads`] kingdom is the land's
/// [`HeldBy`] kingdom) — the predicate form of [`ruled_lands`], for gating
/// context actions like the legend's build/destroy hotkeys.
pub fn rules_land(world: &World, actor: &str, land_id: &str) -> bool {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return false;
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return false;
    };
    let Some(kingdom_e) = world.get::<Leads>(actor_e).map(|l| l.kingdom()) else {
        return false;
    };
    world
        .get::<HeldBy>(land_e)
        .map(|h| h.0 == kingdom_e)
        .unwrap_or(false)
}

/// A fresh v4 UUID for a runtime-built entity, drawn from the seeded `SimRng`.
///
/// ponytail: the id is generated from `SimRng`, not OS entropy, so it keeps the
/// codebase's one-entropy-source invariant (every bit routed through
/// `try_next_u64`). It is a valid v4 UUID string and unique, but deterministic
/// across replays — which is what this sim wants. Format only, no `uuid` crate,
/// no new dependency.
pub(super) fn next_id(world: &mut World) -> String {
    let rng = world.resource::<Game>().ctx.rng.clone();
    let mut b = [0u8; 16];
    {
        let mut r = rng.lock().unwrap();
        let _ = r.try_fill_bytes(&mut b);
    }
    // v4: version nibble 4, variant 10xx.
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15],
    )
}

/// Append `line` to the chronicle.
pub(super) fn note(world: &mut World, line: String) {
    world.resource_mut::<Chronicles>().0.push(line);
}

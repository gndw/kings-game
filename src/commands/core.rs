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
use super::dismiss_army::DismissArmy;
use super::marching::MarchingOrder;
use super::raise_army::RaiseArmy;
use crate::app::Game;
use crate::ecs::{
    BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus, CharacterLeads, KingdomHold,
    LandHasBuildings, LandHeldBy, LandName, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use crate::resources::chronicle::Chronicles;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;
use bevy::prelude::Resource;
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
        r.register(Arc::new(RaiseArmy));
        r.register(Arc::new(DismissArmy));
        r.register(Arc::new(MarchingOrder));
        r
    }
}

impl CommandRegistry {
    /// Register `cmd` at the end of the list.
    pub fn register(&mut self, cmd: Arc<dyn Command>) {
        self.commands.push(cmd);
    }
}

/// The land `actor` rules (can act on): actor → [`CharacterLeads`] → kingdom
/// → its [`KingdomHold`] link (the auto-maintained reverse of the held land's
/// [`LandHeldBy`]). Reads the relationship target with `world::get` so it
/// stays a `&World` read (`world::query` needs `&mut World`); the buildings
/// panel reads the same target.
pub(super) fn ruled_lands(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let Some(kingdom_hold) = world.get::<KingdomHold>(character_leads.kingdom()) else {
        return Vec::new();
    };
    let (Some(string_id), Some(land_name)) = (
        world.get::<StringId>(kingdom_hold.0),
        world.get::<LandName>(kingdom_hold.0),
    ) else {
        return Vec::new();
    };
    vec![(string_id.0.clone(), land_name.0.clone())]
}

/// True if `actor` rules `land_id` (their [`CharacterLeads`] kingdom is the
/// land's [`LandHeldBy`] kingdom) — the predicate form of [`ruled_lands`], for
/// gating context actions like the actions panel's build/destroy hotkeys.
pub fn rules_land(world: &World, actor: &str, land_id: &str) -> bool {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return false;
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return false;
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return false;
    };
    world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom() == character_leads.kingdom())
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
pub(crate) fn note(world: &mut World, line: String) {
    world.resource_mut::<Chronicles>().0.push(line);
}

// --- building-levy helpers ---------------------------------------------------
// The raise / dismiss pair share three operations on `BuildingLevy` and
// `BuildingIsRaised`: sum the available pool, drain it to the army, and
// distribute it back. Kept here so both commands reach for the same code
// path; not in `game/yields.rs` because they're command-internal.

/// Sum every ACTIVE building's `BuildingLevy` on `land_e`. Returns
/// `(total, has_any)` — `has_any` distinguishes "no ACTIVE buildings" from
/// "ACTIVE buildings exist but their pools are all drained". The raise
/// gate is `has_any && total > 0`; the second is implied by the first
/// (`has_any` requires at least one contributing building), but kept
/// explicit so a future `BuildingLevy` default of `0` doesn't slip past
/// the check.
pub(super) fn available_levy(world: &World, land_e: Entity) -> (u64, bool) {
    let Some(land_has_buildings) = world.get::<LandHasBuildings>(land_e) else {
        return (0, false);
    };
    let mut total: u64 = 0;
    let mut any = false;
    for b_e in land_has_buildings.iter() {
        if !is_active_building(world, b_e) {
            continue;
        }
        if let Some(building_levy) = world.get::<BuildingLevy>(b_e) {
            total += building_levy.0 as u64;
            any = true;
        }
    }
    (total, any)
}

/// Drain every ACTIVE building's `BuildingLevy` on `land_e` to `0` and flag
/// it as raised. Called by `RaiseArmy` after the army bundle is spawned;
/// `BuildingLevy == 0` plus `BuildingIsRaised == true` is the "this
/// building's levy is currently in an army" state.
pub(super) fn drain_buildings(world: &mut World, land_e: Entity) {
    // Snapshot entities, drop the borrow before any `get_mut` — see
    // `distribute_levy_back` for the rationale.
    let entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
        Some(land_has_buildings) => land_has_buildings.iter().collect(),
        None => return,
    };
    for b_e in entities {
        if !is_active_building(world, b_e) {
            continue;
        }
        if let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e) {
            building_levy.0 = 0;
        }
        if let Some(mut building_is_raised) = world.get_mut::<BuildingIsRaised>(b_e) {
            building_is_raised.0 = true;
        }
    }
}

/// Distribute `army_levy` back into each ACTIVE building's `BuildingLevy`
/// on `land_e`, capped at the def's `levy`. Sets `BuildingIsRaised` back
/// to `false` for every ACTIVE building on the land (a no-op for ones that
/// weren't raised — defensive against torn edge cases). Levy that won't fit
/// in any building (rare — only if the army outgrew the buildings' caps) is
/// dropped, since there's no "overflow" building to pour into.
pub(super) fn distribute_levy_back(world: &mut World, land_e: Entity, army_levy: u64) {
    // Snapshot entities, then drop the borrow before any `get_mut` —
    // holding `&LandHasBuildings` across the mutation loop would conflict.
    let entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
        Some(land_has_buildings) => land_has_buildings.iter().collect(),
        None => return,
    };
    let mut remaining = army_levy;
    for b_e in entities {
        if !is_active_building(world, b_e) {
            continue;
        }
        // Cap lookup in its own scope so `defs` drops before the `get_mut`
        // below — otherwise the immutable `defs` borrow collides with the
        // mutable `get_mut` borrow of `world`.
        let cap = {
            let defs = world.resource::<BuildingDefs>();
            world
                .get::<BuildingOf>(b_e)
                .and_then(|bo| defs.get(&bo.0).map(|d| d.levy))
                .unwrap_or(0)
        };
        if remaining > 0
            && cap > 0
            && let Some(mut building_levy) = world.get_mut::<BuildingLevy>(b_e)
        {
            // Pour up to `cap` (or the rest of the army's levy, whichever is
            // smaller) into this building's pool. Order of iteration isn't
            // weighted — archetype order is deterministic, so the "first
            // building" always wins any overflow race. A future
            // weighted/proportional fill is the obvious upgrade.
            let space = cap.saturating_sub(building_levy.0);
            let add = space.min(remaining as u32);
            building_levy.0 += add;
            remaining = remaining.saturating_sub(add as u64);
        }
        if let Some(mut building_is_raised) = world.get_mut::<BuildingIsRaised>(b_e) {
            building_is_raised.0 = false;
        }
    }
}

/// True if `b_e` is a building entity with status `Active`. Used by the
/// levy helpers so they only touch the buildings that count toward raising.
fn is_active_building(world: &World, b_e: Entity) -> bool {
    world
        .get::<BuildingStatus>(b_e)
        .map(|status| *status == BuildingStatus::Active)
        .unwrap_or(false)
}

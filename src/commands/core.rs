//! Shared helpers every command reaches for: a fresh id, a chronicle line,
//! the "lands this actor rules" walk, and the building-levy pool operations
//! the raise / dismiss pair share.
//!
//! Also owns the [`BaseCommand`] trait every command file implements and
//! the [`spawn_command`] orchestrator the v2 palette calls to populate
//! the panel. For now only [`ConstructBuilding`](crate::commands::construct_building::ConstructBuilding)
//! adopts the trait; the others come in turn.

use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

use crate::app::Game;
use crate::commands::construct_building::ConstructBuilding;
use crate::commands::dismiss_army::DismissArmy;
use crate::commands::enforce_demands::EnforceDemands;
use crate::commands::lay_siege::LaySiege;
use crate::commands::marching::MarchingOrder;
use crate::commands::raise_army::RaiseArmy;
use crate::commands::declare_war::DeclareWar;
use crate::commands::destroy_building::DestroyBuilding;
use crate::ecs::{
    BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus, CharacterLeads, KingdomHold,
    LandHasBuildings, LandHeldBy, LandName, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use crate::resources::chronicle::Chronicles;
use bevy::prelude::RelationshipTarget;
use bevy::prelude::Resource;
use bevy::prelude::With;
use rand::TryRng;

/// One selectable row in a command's step list. `label` is what the player
/// sees; `value` is the id the command reads back in a later step or in the
/// command's effect.
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

/// The uniform interface every player command implements. Each command
/// file supplies its own UI, so the palette orchestrator ([`spawn_command`])
/// can drive every command through this surface without knowing the
/// concrete struct. The command is responsible for returning the entities
/// it spawned so the palette can track and despawn them, and for re-styling
/// each row in response to selection.
pub trait BaseCommand: Send + Sync {
    /// Stable, unique string id for the command (e.g.
    /// `"command:construct_building"`). Stored alongside the command
    /// instance in [`CommandEntry`] so the orchestrator + future
    /// selection / dispatch layer can look the command up by name.
    fn get_command_id(&self) -> &'static str;
    /// Spawn the command's UI into the palette's list panel. `parent` is
    /// the list entity — the command should `ChildOf` its row to it and
    /// add whatever child `Text` / `Node` elements the command wants.
    /// Each command decides its own visual layout so the panel can host a
    /// heterogeneous set of pickers.
    ///
    /// Returns a `(entities, is_executed)` pair:
    /// - `entities`: every entity the command spawned (in display order),
    ///   so the palette can despawn them on close and resolve a click
    ///   back to a row.
    /// - `is_executed`: `true` when the player's running choices already
    ///   carry enough information for this command to act on (e.g. a
    ///   `construct_building` that has both a land pick and a building
    ///   pick). The orchestrator uses this to decide whether to close
    ///   the panel.
    ///
    /// `choices` is the player's running selection list
    /// (`(key, value)` pairs, e.g. `("command", "command:construct_building")`).
    /// The command inspects it to decide what to show: no `"command"` key
    /// means first time / fresh, a matching key means the command is the
    /// current pick, a non-matching key means another command was picked.
    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool);
    /// Re-style one of the entities the command previously spawned.
    /// `entity` is expected to be one of the entities returned by the
    /// last call to `spawn_command` for this command. `is_selected`
    /// indicates whether the palette's cursor is currently on this row;
    /// the command decides what that visually means (background swap,
    /// border, glow).
    fn update(&self, entity: Entity, is_selected: bool, world: &mut World);
}

/// One entry in [`CommandContext`]: a stable id paired with the
/// [`BaseCommand`] instance it labels. `id` mirrors what
/// [`BaseCommand::get_command_id`] returns at runtime, captured at
/// [`startup`] so the orchestrator and any future dispatch layer can
/// match by name without holding a borrow on the command struct.
pub struct CommandEntry {
    pub id: &'static str,
    pub cmd: &'static dyn BaseCommand,
}

/// Runtime roster of every command the palette can surface. Populated by
/// [`startup`] from the concrete command structs and read by [`spawn_command`]
/// and [`update`]. Replaces the old `const COMMANDS` table so the roster
/// can grow without touching the orchestrator.
#[derive(Resource, Default)]
pub struct CommandContext {
    pub commands: Vec<CommandEntry>,
}

pub fn startup(world: &mut World) {
    let commands = vec![
        CommandEntry {
            id: ConstructBuilding.get_command_id(),
            cmd: &ConstructBuilding,
        },
        CommandEntry {
            id: DestroyBuilding.get_command_id(),
            cmd: &DestroyBuilding,
        },
        CommandEntry {
            id: RaiseArmy.get_command_id(),
            cmd: &RaiseArmy,
        },
        CommandEntry {
            id: DismissArmy.get_command_id(),
            cmd: &DismissArmy,
        },
        CommandEntry {
            id: MarchingOrder.get_command_id(),
            cmd: &MarchingOrder,
        },
        CommandEntry {
            id: DeclareWar.get_command_id(),
            cmd: &DeclareWar,
        },
        CommandEntry {
            id: LaySiege.get_command_id(),
            cmd: &LaySiege,
        },
        CommandEntry {
            id: EnforceDemands.get_command_id(),
            cmd: &EnforceDemands,
        },
    ];
    world.insert_resource(CommandContext { commands });
}

/// One row in the runtime command roster: a stable id paired with a
/// reference to the [`BaseCommand`] instance it labels. The
/// orchestrator ([`spawn_command`]) iterates this list at spawn time.
/// Adding a new command is one line in [`startup`] — the spawn path
/// picks it up automatically.

/// Orchestrator: find the panel's list and let every entry in
/// [`CommandContext`] spawn its own UI into it. Returns
/// `(entities, is_executed)`. `entities` is the flat list of every
/// entity every command produced, in roster order. `is_executed` is
/// `true` if any of those commands reported it had enough information
/// in `choices` to act (e.g. a `construct_building` with both a land
/// pick and a building pick); the caller uses it to decide whether to
/// close the panel.
///
/// `choices` is forwarded to every command's
/// [`BaseCommand::spawn_command`] so each row can decide what to show
/// based on the player's running selection.
pub fn spawn_command(
    world: &mut World,
    choices: &[(String, String)],
) -> (Vec<Entity>, bool) {
    let Some(list) = world
        .query_filtered::<Entity, With<crate::ui::command_menu::CommandMenuUIList>>()
        .iter(world)
        .next()
    else {
        return (Vec::new(), false);
    };
    // Snapshot the cmd refs so the immutable borrow on `CommandContext`
    // drops before we touch `world` mutably.
    let cmds: Vec<&'static dyn BaseCommand> = world
        .resource::<CommandContext>()
        .commands
        .iter()
        .map(|e| e.cmd)
        .collect();
    let mut entities = Vec::new();
    let mut any_executed = false;
    for cmd in cmds {
        let (spawned, executed) = cmd.spawn_command(world, list, choices);
        entities.extend(spawned);
        any_executed = any_executed || executed;
    }
    (entities, any_executed)
}

/// Re-style a single spawned entity to match the current selection
/// state. The entity is expected to carry a
/// [`CommandHasId`](crate::ui::command_menu::CommandHasId) (set by the
/// spawning command); the orchestrator reads it, looks the command up
/// in [`CommandContext`] by id, and delegates to that command's
/// `update`. No-op for entities that aren't palette rows.
pub fn update(entity: Entity, is_selected: bool, world: &mut World) {
    let Some(command_id) = world
        .get::<crate::ui::command_menu::CommandHasId>(entity)
        .map(|c| c.0.clone())
    else {
        return;
    };
    // Snapshot the matched cmd so the `CommandContext` borrow drops
    // before `cmd.update` takes `&mut World`.
    let cmd: Option<&'static dyn BaseCommand> = world
        .resource::<CommandContext>()
        .commands
        .iter()
        .find(|e| e.id == command_id)
        .map(|e| e.cmd);
    if let Some(cmd) = cmd {
        cmd.update(entity, is_selected, world);
    }
}

/// The lands `actor` rules (can act on): walks
/// `actor → CharacterLeads → kingdoms → KingdomHold → land`. With the
/// multi-kingdom model the player can lead several kingdoms, so this
/// collects every ruled land across every kingdom the player leads —
/// `ruled_lands` is the union, not the held land of "the" kingdom.
/// Reads the relationship target with `world::get` so it stays a
/// `&World` read (`world::query` needs `&mut World`); the buildings
/// panel reads the same targets.
pub(super) fn ruled_lands(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_hold) = world.get::<KingdomHold>(*kingdom_e) else {
            continue;
        };
        let (Some(string_id), Some(land_name)) = (
            world.get::<StringId>(kingdom_hold.0),
            world.get::<LandName>(kingdom_hold.0),
        ) else {
            continue;
        };
        out.push((string_id.0.clone(), land_name.0.clone()));
    }
    out
}

/// True if `actor` rules `land_id` — the predicate form of [`ruled_lands`]
/// for gating context actions like the actions panel's build/destroy
/// hotkeys. Multi-kingdom: any of the actor's kingdoms ruling the land
/// counts.
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
    let Some(land_held_by) = world.get::<LandHeldBy>(land_e) else {
        return false;
    };
    let land_kingdom = land_held_by.kingdom();
    character_leads
        .kingdoms()
        .iter()
        .any(|&k| k == land_kingdom)
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
/// building's levy is currently in an army" state. Returns the affected
/// buildings so the caller can fire per-building `OnBuildingUpdated` events.
pub(super) fn drain_buildings(world: &mut World, land_e: Entity) -> Vec<Entity> {
    // Snapshot entities, drop the borrow before any `get_mut` — see
    // `distribute_levy_back` for the rationale.
    let entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
        Some(land_has_buildings) => land_has_buildings.iter().collect(),
        None => return Vec::new(),
    };
    let mut drained = Vec::new();
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
        drained.push(b_e);
    }
    drained
}

/// Distribute `army_levy` back into each ACTIVE building's `BuildingLevy`
/// on `land_e`, capped at the def's `levy`. Sets `BuildingIsRaised` back
/// to `false` for every ACTIVE building on the land (a no-op for ones that
/// weren't raised — defensive against torn edge cases). Levy that won't fit
/// in any building (rare — only if the army outgrew the buildings' caps) is
/// dropped, since there's no "overflow" building to pour into. Returns only
/// the buildings that were actually raised (so callers can fire per-building
/// `OnBuildingUpdated` for real state transitions, not the defensive flips).
pub(super) fn distribute_levy_back(
    world: &mut World,
    land_e: Entity,
    army_levy: u64,
) -> Vec<Entity> {
    // Snapshot entities, then drop the borrow before any `get_mut` —
    // holding `&LandHasBuildings` across the mutation loop would conflict.
    let entities: Vec<Entity> = match world.get::<LandHasBuildings>(land_e) {
        Some(land_has_buildings) => land_has_buildings.iter().collect(),
        None => return Vec::new(),
    };
    let mut remaining = army_levy;
    let mut dismissed = Vec::new();
    for b_e in entities {
        if !is_active_building(world, b_e) {
            continue;
        }
        // Snapshot the previous raised state so we only fire events for
        // buildings that genuinely transitioned raised → not-raised.
        let was_raised = world
            .get::<BuildingIsRaised>(b_e)
            .map(|bir| bir.0)
            .unwrap_or(false);
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
        if was_raised {
            dismissed.push(b_e);
        }
    }
    dismissed
}

/// True if `b_e` is a building entity with status `Active`. Used by the
/// levy helpers so they only touch the buildings that count toward raising.
fn is_active_building(world: &World, b_e: Entity) -> bool {
    world
        .get::<BuildingStatus>(b_e)
        .map(|status| *status == BuildingStatus::Active)
        .unwrap_or(false)
}

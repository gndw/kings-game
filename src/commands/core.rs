//! Shared helpers every command reaches for: a fresh id, a chronicle line,
//! the "lands this actor rules" walk, and the building-levy pool operations
//! the raise / dismiss pair share.
//!
//! Also owns the [`BaseCommand`] trait every command file implements and
//! the [`spawn_command`] orchestrator the v2 palette calls to populate
//! the panel.

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
    LandHasBuildings, LandName, Registry, StringId,
};
use crate::ecs::army::{ArmyHasMarching, ArmyHasSiege, ArmyMarching, ArmyStatus};
use crate::ecs::marching::{MarchingArrivedDate, MarchingOnRoad, MarchingToLand};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::resources::buildings::BuildingDefs;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::events::OnErrorOccured;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue};
use bevy::prelude::RelationshipTarget;
use bevy::prelude::Resource;
use bevy::prelude::With;
use bevy::prelude::*;
use rand::TryRng;

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

/// Fire [`OnErrorOccured`] with `message`. The validation side of
/// commands reaches for this so the player sees the failure in a modal
/// popup (`ui::error`) rather than buried in the chronicle scroll.
/// Game-event lines (construction begun, army arrived, war declared)
/// reach the chronicle via events observed in
/// [`crate::chronicles`].
pub(crate) fn error(world: &mut World, message: String) {
    world.trigger(OnErrorOccured { message });
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

// --- per-land / per-army read helpers -------------------------------------
// The land and army pickers in every command need a handful of stats
// (yield, available levy, army status, …) that come from the world in
// `&World` form. Each helper is a thin `world.get` walk that mirrors
// the corresponding Bevy-system logic so the picker functions stay
// readable and don't repeat the same loops.

/// `(net_gold_per_month, total_levy)` for every ACTIVE building on
/// `land_e`. Mirrors [`crate::game::yields::sum_land_yield`] but reads
/// via `world.get` so the picker functions (which take `&mut World`)
/// can call it without rebuilding the Bevy `Query` plumbing. Used by
/// the construct / destroy land pickers to show what each ruled land
/// earns and drafts.
pub(super) fn land_yield(world: &World, land_e: Entity) -> (i64, u64) {
    let Some(land_has_buildings) = world.get::<LandHasBuildings>(land_e) else {
        return (0, 0);
    };
    let defs = world.resource::<BuildingDefs>();
    let (mut gold, mut levy) = (0i64, 0u64);
    for b_e in land_has_buildings.iter() {
        let Some(building_of) = world.get::<BuildingOf>(b_e) else {
            continue;
        };
        let active = world
            .get::<BuildingStatus>(b_e)
            .map(|status| *status == BuildingStatus::Active)
            .unwrap_or(false);
        if !active {
            continue;
        }
        if let Some(d) = defs.get(&building_of.0) {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
        }
    }
    (gold, levy)
}

/// One-line status text for an army: `idle`, `→ <land> in <days>d`,
/// or `sieging <land> (<progress>%)`. Mirrors the ARMIES panel's line
/// format so the picker rows read identically. Reads everything via
/// `world.get` so it stays a `&World` helper. `None` when the army's
/// required components are missing — the caller skips the row.
pub(super) fn army_status_text(
    world: &World,
    army_e: Entity,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let status = world.get::<ArmyStatus>(army_e).copied().unwrap_or(ArmyStatus::Idle);
    match status {
        ArmyStatus::Idle => Some("idle".into()),
        ArmyStatus::Marching => {
            // Walk the queue: find the final destination (last hop's
            // `MarchingToLand` land name) and the total days remaining
            // (today's ordinal to the OnRoute arrival plus each
            // subsequent Scheduled hop's `RoadDistanceDays`).
            let queue = world.get::<ArmyHasMarching>(army_e)?;
            let hops: Vec<Entity> = queue.iter().collect();
            let current_marching = world
                .get::<ArmyMarching>(army_e)
                .copied()
                .map(|m| m.0);
            let today_ord = date.ordinal(calendar);

            let on_route_days: i64 = current_marching
                .and_then(|cur| world.get::<MarchingArrivedDate>(cur))
                .and_then(|d| d.0)
                .map(|arrived| (arrived.ordinal(calendar) - today_ord).max(0))
                .unwrap_or(0);

            let mut total_days: i64 = 0;
            for &hop in &hops {
                let Some(marching_on_road) = world.get::<MarchingOnRoad>(hop) else {
                    continue;
                };
                if let Some(road_distance_days) = world.get::<RoadDistanceDays>(marching_on_road.0)
                {
                    total_days += road_distance_days.0 as i64;
                }
            }
            if current_marching.is_some()
                && let Some(cur) = current_marching
                && let Some(marching_on_road) = world.get::<MarchingOnRoad>(cur)
                && let Some(road_distance_days) = world.get::<RoadDistanceDays>(marching_on_road.0)
            {
                total_days -= road_distance_days.0 as i64;
            }
            total_days += on_route_days;

            let final_dest = hops
                .last()
                .and_then(|&h| world.get::<MarchingToLand>(h))
                .and_then(|to| world.get::<LandName>(to.0))
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "?".into());
            Some(format!("→ {final_dest} in {total_days}d"))
        }
        ArmyStatus::Sieging => {
            let siege_e = world.get::<ArmyHasSiege>(army_e)?.siege();
            let progress = world
                .get::<SiegeProgress>(siege_e)
                .map(|siege_progress| siege_progress.0)
                .unwrap_or(0);
            Some(format!("sieging ({progress}%)"))
        }
    }
}

// --- palette row styling --------------------------------------------------
// Every command's picker row is the same shape: a padded card with the
// same per-row colours, a name (plus an optional smaller description)
// on the left, and up to two right-aligned stat cells. The constants
// and the builder live here so commands don't redeclare them and the
// styling stays in one place. The selection tint is also shared — every
// command's `update` is a one-liner now.

/// Per-row background in the palette. One shade lighter than the panel.
pub(super) const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
pub(super) const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
pub(super) const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);
/// Width of each right-aligned stat column.
pub(super) const STAT_W: f32 = 96.0;
/// Default name colour (the regular pickable row).
pub(super) const NAME_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
/// Name colour when the row's choice is unavailable — cannot afford, no
/// road route, gate not met. The row is still in the list (no disabled
/// state); the suffix on the name explains why.
pub(super) const HINT_RED: Color = Color::srgb(0.92, 0.40, 0.40);
/// Description-line colour (smaller font under the name).
pub(super) const DESC_COLOR: Color = Color::srgba(0.78, 0.78, 0.82, 0.95);
/// Default stat colour.
pub(super) const STAT_COLOR: Color = Color::srgba(0.92, 0.92, 0.95, 1.0);
/// Stat colour when the value is empty / the slot doesn't apply.
pub(super) const STAT_DIM: Color = Color::srgba(0.55, 0.55, 0.60, 0.85);

/// Swap the row's background between the unselected/selected shades.
/// Called from every command's `update`; centralised so the palette
/// styling stays in one place.
pub(super) fn set_row_selected(world: &mut World, entity: Entity, is_selected: bool) {
    let bg = if is_selected {
        ROW_PANEL_SELECTED
    } else {
        ROW_PANEL
    };
    if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
        background.0 = bg;
    }
}

/// Spawn one picker row. `name` is the main line; `description` is an
/// optional smaller line under it (effect summary for `ConstructBuilding`
/// building rows, defender/ruler detail for `LaySiege` army rows).
/// `stat1` / `stat2` are optional right-aligned cells — pass `None` to
/// leave the slot empty. `key_value` carries the step's
/// `(CommandHasKey, CommandHasValue)` for step rows; `None` for the
/// command's own top-level row.
///
/// The row is also stamped with [`CommandHasQueryable`] (the search
/// key — same as the displayed `name`) and [`RowNameText`] (the name
/// text child, so the palette can recolour the name alone when the row
/// is grayed by a search filter).
pub(super) fn picker_row(
    world: &mut World,
    parent: Entity,
    command_id: &str,
    key_value: Option<(String, String)>,
    name: &str,
    name_color: Color,
    description: Option<&str>,
    stat1: Option<(&str, Color)>,
    stat2: Option<(&str, Color)>,
) -> Entity {
    let mut entity = world.spawn((
        Node {
            width: percent(100),
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(ROW_PANEL),
        BorderColor::all(ROW_BORDER),
        ChildOf(parent),
        CommandHasId(command_id.to_string()),
        crate::ui::command_menu::CommandHasQueryable(name.to_string()),
    ));
    if let Some((k, v)) = key_value {
        entity.insert((CommandHasKey(k), CommandHasValue(v)));
    }
    let row = entity.id();
    let mut name_text_entity: Option<Entity> = None;
    world.entity_mut(row).with_children(|c| {
        // Name column — fills remaining width.
        let mut name_col = c.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            ..default()
        });
        name_col.with_children(|name_col_cmd| {
            let name_text = name_col_cmd
                .spawn((
                    Text::new(name.to_string()),
                    TextFont::from_font_size(16.0),
                    TextColor(name_color),
                ))
                .id();
            name_text_entity = Some(name_text);
            if let Some(desc) = description {
                name_col_cmd.spawn((
                    Text::new(desc.to_string()),
                    TextFont::from_font_size(11.0),
                    TextColor(DESC_COLOR),
                ));
            }
        });
        if let Some((text, color)) = stat1 {
            c.spawn((
                Text::new(text.to_string()),
                TextFont::from_font_size(14.0),
                TextColor(color),
                TextLayout::justify(Justify::Right),
                Node {
                    width: px(STAT_W),
                    ..default()
                },
            ));
        }
        if let Some((text, color)) = stat2 {
            c.spawn((
                Text::new(text.to_string()),
                TextFont::from_font_size(14.0),
                TextColor(color),
                TextLayout::justify(Justify::Right),
                Node {
                    width: px(STAT_W),
                    ..default()
                },
            ));
        }
    });
    if let Some(name_text) = name_text_entity {
        world
            .entity_mut(row)
            .insert(crate::ui::command_menu::RowNameText(name_text));
    }
    row
}

//! Shared helpers every command reaches for, plus the `BaseCommand` trait
//! every command file implements and the `spawn_command` orchestrator.

use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

use crate::app::Game;
use crate::commands::construct_building::ConstructBuilding;
use crate::commands::dismiss_army::DismissArmy;
use crate::commands::enforce_demands::EnforceDemands;
use crate::commands::gift_gold::GiftGold;
use crate::commands::lay_siege::LaySiege;
use crate::commands::marching::MarchingOrder;
use crate::commands::raise_army::RaiseArmy;
use crate::commands::declare_war::DeclareWar;
use crate::commands::destroy_building::DestroyBuilding;
use crate::ecs::{
    BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus, KingdomGold, KingdomHold,
    LandHasBuildings, LandName, Registry, StringId,
};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
use crate::ecs::character::{
    Memory, MemoryCreatedDate, MemoryKind, MemoryOfCharacter, MemoryTowardCharacter,
    MemoryUntilDate,
};
use crate::ecs::army::{ArmyHasMarching, ArmyHasSiege, ArmyLevy, ArmyMarching, ArmyMaxLevy, ArmyStatus};
use crate::ecs::marching::{MarchingArrivedDate, MarchingOnRoad, MarchingToLand};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::resources::buildings::BuildingDefs;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::observers::{OnErrorOccurred, OnGoldGifted};
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue};
use bevy::prelude::RelationshipTarget;
use bevy::prelude::Resource;
use bevy::prelude::With;
use bevy::prelude::*;
use rand::TryRng;

/// The uniform interface every player command implements.
pub trait BaseCommand: Send + Sync {
    /// Stable, unique string id for the command (e.g. `"command:construct_building"`).
    fn get_command_id(&self) -> &'static str;
    /// Spawn the command's UI into the palette's list. Returns `(entities, is_executed)`:
    /// `entities` is what to track/despawn, `is_executed` is `true` when the choices
    /// already carry enough info for the command to act on.
    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool);
    /// Re-style one of the entities previously spawned by `spawn_command`.
    fn update(&self, entity: Entity, is_selected: bool, world: &mut World);
}

/// One entry in `CommandContext`: a stable id paired with the `BaseCommand` instance.
pub struct CommandEntry {
    pub id: &'static str,
    pub cmd: &'static dyn BaseCommand,
}

/// Runtime roster of every command the palette can surface.
#[derive(Resource, Default)]
pub struct CommandContext {
    pub commands: Vec<CommandEntry>,
}

pub fn startup(world: &mut World) {
    let commands = vec![
        CommandEntry { id: ConstructBuilding.get_command_id(), cmd: &ConstructBuilding },
        CommandEntry { id: DestroyBuilding.get_command_id(), cmd: &DestroyBuilding },
        CommandEntry { id: RaiseArmy.get_command_id(), cmd: &RaiseArmy },
        CommandEntry { id: DismissArmy.get_command_id(), cmd: &DismissArmy },
        CommandEntry { id: MarchingOrder.get_command_id(), cmd: &MarchingOrder },
        CommandEntry { id: DeclareWar.get_command_id(), cmd: &DeclareWar },
        CommandEntry { id: LaySiege.get_command_id(), cmd: &LaySiege },
        CommandEntry { id: EnforceDemands.get_command_id(), cmd: &EnforceDemands },
        CommandEntry { id: GiftGold.get_command_id(), cmd: &GiftGold },
    ];
    world.insert_resource(CommandContext { commands });
}

/// Orchestrator: let every entry in `CommandContext` spawn its own UI into the panel's list.
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
    // Snapshot the cmd refs so the immutable borrow on `CommandContext` drops before mutation.
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

/// Re-style a single spawned entity by looking up its owning command and delegating to `update`.
pub fn update(entity: Entity, is_selected: bool, world: &mut World) {
    let Some(command_id) = world
        .get::<crate::ui::command_menu::CommandHasId>(entity)
        .map(|c| c.0.clone())
    else {
        return;
    };
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

/// The lands `actor` rules: walks `actor → get_character_ruled_kingdoms → KingdomHold`,
/// collecting every ruled land across every kingdom the actor leads.
pub(super) fn ruled_lands(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in get_character_ruled_kingdoms(world, actor_e) {
        let Some(kingdom_hold) = world.get::<KingdomHold>(kingdom_e) else {
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

/// A fresh v4 UUID drawn from the seeded `SimRng` (one-entropy-source invariant).
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

/// Fire `OnErrorOccurred` with `message`; the error popup shows it as a modal.
pub(crate) fn error(world: &mut World, message: String) {
    world.trigger(OnErrorOccurred { message });
}

// --- building-levy helpers ---------------------------------------------------
// Sum the available pool, drain it to the army, and distribute it back.

/// Sum every ACTIVE building's `BuildingLevy` on `land_e`. Returns `(total, has_any)`.
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

/// Distribute `army_levy` back into each ACTIVE building's `BuildingLevy` on `land_e`,
/// capped at the def's `levy`. Returns buildings that were actually raised.
pub(super) fn distribute_levy_back(
    world: &mut World,
    land_e: Entity,
    army_levy: u64,
) -> Vec<Entity> {
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
        let was_raised = world
            .get::<BuildingIsRaised>(b_e)
            .map(|bir| bir.0)
            .unwrap_or(false);
        // Cap lookup in its own scope so `defs` drops before the `get_mut` below.
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

/// True if `b_e` is a building entity with status `Active`.
fn is_active_building(world: &World, b_e: Entity) -> bool {
    world
        .get::<BuildingStatus>(b_e)
        .map(|status| *status == BuildingStatus::Active)
        .unwrap_or(false)
}

// --- per-land / per-army read helpers -------------------------------------
// Thin `world.get` walks mirroring the corresponding Bevy-system logic.

/// `(net_gold_per_month, total_levy)` for every ACTIVE building on `land_e`.
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

/// One-line status text for an army: `idle`, `→ <land> in <days>d`, `sieging (<progress>%)`,
/// or `raising <levy>/<max>`. `None` when components are missing.
pub(super) fn army_status_text(
    world: &World,
    army_e: Entity,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let status = world.get::<ArmyStatus>(army_e).copied().unwrap_or(ArmyStatus::Idle);
    match status {
        ArmyStatus::Idle => Some("idle".into()),
        ArmyStatus::Raising => {
            let levy = world.get::<ArmyLevy>(army_e).map(|x| x.0).unwrap_or(0);
            let max = world.get::<ArmyMaxLevy>(army_e).map(|x| x.0).unwrap_or(0);
            Some(format!("raising {levy}/{max}"))
        }
        ArmyStatus::Marching => {
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
// Shared row colours and a builder so commands don't redeclare them.

pub(super) const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
pub(super) const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
pub(super) const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);
pub(super) const STAT_W: f32 = 96.0;
pub(super) const NAME_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
/// Name colour when the row's choice is unavailable (can't afford, no route, gate not met).
pub(super) const HINT_RED: Color = Color::srgb(0.92, 0.40, 0.40);
pub(super) const DESC_COLOR: Color = Color::srgba(0.78, 0.78, 0.82, 0.95);
pub(super) const STAT_COLOR: Color = Color::srgba(0.92, 0.92, 0.95, 1.0);
pub(super) const STAT_DIM: Color = Color::srgba(0.55, 0.55, 0.60, 0.85);

/// Swap the row's background between the unselected/selected shades.
pub(super) fn set_row_selected(world: &mut World, entity: Entity, is_selected: bool) {
    let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
    if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
        background.0 = bg;
    }
}

/// Spawn one picker row with optional description and up to two right-aligned stat cells.
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
                Node { width: px(STAT_W), ..default() },
            ));
        }
        if let Some((text, color)) = stat2 {
            c.spawn((
                Text::new(text.to_string()),
                TextFont::from_font_size(14.0),
                TextColor(color),
                TextLayout::justify(Justify::Right),
                Node { width: px(STAT_W), ..default() },
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

// --- shared gold transfer helper --------------------------------------------
// Phase 2 (mutate) and phase 3 (spawn memory) of `gift_gold::gift`; the event
// resolver uses the same dance. Callers do their own validation (sufficient
/// gold on `from_e`'s primary kingdom, no active `ReceivedGold` memory on
/// `to_e`) and pass the computed memory-expiry `until`.
///
/// Gold is a realm treasury. A personal gift debits the giver's primary
/// kingdom and *does not credit the recipient's kingdom* — a coin handed to
/// a stranger leaves the giver's treasury and isn't re-booked anywhere. The
/// recipient gains a memory of the gift (which boosts their opinion of the
/// giver for the memory's lifetime), but no tracked gold lands on them.
/// Fires [`OnGoldGifted`] so the chronicle observer writes a line.
///
/// Validation is the caller's job; this helper assumes `from_e` can afford
/// `amount` and `to_e` has no active `ReceivedGold` memory (matching
/// `gift_gold::gift`'s pre-checks). The two call sites today are
/// `commands::gift_gold::gift` and `game::presenting_event::resolve_choice`.
///
/// ponytail: the helper lives here because both call sites work through
/// `commands::core`. If a third caller appears (e.g. a tribute command), keep
/// it here — the right refactor would be a `src/helper/gift_helper.rs` module
/// only if the helper grows contract surface (more effects than
/// gold+memory).
pub(crate) fn transfer_with_gold_memory(
    world: &mut World,
    from_e: Entity,
    to_e: Entity,
    amount: i64,
    until: Date,
) {
    // Phase 2: debit the giver's primary kingdom. Debt is real.
    if let Some(mut from_g) = world
        .get_mut::<KingdomGold>(get_character_primary_kingdom(world, from_e))
    {
        from_g.0 -= amount;
    }
    // No credit side — see the doc comment above.

    // Phase 3: spawn the memory and register it for `from_e → to_e` lookup.
    let today = *world.resource::<Date>();
    let from_id = id_string_for(world, from_e);
    let to_id = id_string_for(world, to_e);
    let memory_id = format!("memory-{from_id}-{to_id}-{today}");
    let memory_e = world
        .spawn((
            StringId(memory_id.clone()),
            Memory,
            MemoryOfCharacter(to_e),
            MemoryTowardCharacter(from_e),
            MemoryCreatedDate(today),
            MemoryUntilDate(until),
            MemoryKind::ReceivedGold { amount },
        ))
        .id();
    world
        .resource_mut::<Registry>()
        .by_id
        .insert(memory_id, memory_e);

    world.trigger(OnGoldGifted {
        from: from_e,
        to: to_e,
        amount,
    });
}

/// The first kingdom `character_e` rules, or `character_e` itself as a
/// fallback when none exists (debit becomes a no-op via `get_mut`).
fn get_character_primary_kingdom(world: &World, character_e: Entity) -> Entity {
    get_character_ruled_kingdoms(world, character_e)
        .first()
        .copied()
        .unwrap_or(character_e)
}

/// StringId lookup with a stable fallback so memory ids remain unique even
/// when an entity somehow lacks a `StringId` (every game entity should carry
/// one — see architecture.md, "Key invariants" — but the memory id format
/// predates the invariant, so we keep a defensive fallback here).
fn id_string_for(world: &World, e: Entity) -> String {
    world
        .get::<StringId>(e)
        .map(|s| s.0.clone())
        .unwrap_or_else(|| format!("e{e:?}"))
}

/// Every alive character in deterministic registry order. Walks
/// `Registry.by_id`, filters to `char-*` ids, and skips the actor. Used by
/// event attendee pickers and reusable anywhere a roster of "everyone" is
/// needed.
pub(crate) fn alive_characters_excluding(
    world: &World,
    actor: Entity,
) -> Vec<(String, Entity)> {
    let registry = world.resource::<Registry>();
    let mut out: Vec<(String, Entity)> = registry
        .by_id
        .iter()
        .filter(|(id, _)| id.starts_with("char-"))
        .map(|(id, e)| (id.clone(), *e))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter()
        .filter(|(_, e)| {
            *e != actor
                && world
                    .get::<crate::ecs::character::CharacterIsAlive>(*e)
                    .map(|a| a.0)
                    .unwrap_or(false)
        })
        .collect()
}

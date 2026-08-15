//! The script-callable surface. Every event function takes a single `world`
//! argument (a Rhai map). To expose new data to scripts, add a field to the
//! map in `build_world_view`; to expose new actions, register a method on
//! `ScriptCtx` below. Neither change breaks existing scripts.
//!
//! `ScriptCtx` is a thin wrapper around a raw pointer to `World`. It exists
//! for the lifetime of one script call — the runtime constructs one, the
//! engine holds it as a `Dynamic` inside `world.ctx`, and the engine drops
//! it when `call_fn` returns. While the wrapper is alive, no other code
//! touches the world.

use crate::app::Game;
use crate::commands::core::{alive_characters_excluding, transfer_with_gold_memory};
use crate::ecs::Registry;
use crate::ecs::character::{
    CharacterGold, CharacterIsAlive, CharacterLevy, CharacterName, CharacterOfHouse,
};
use crate::resources::chronicle::Chronicles;
use crate::resources::date::Date;
use bevy::prelude::*;
use rhai::{Array, Dynamic, Engine, Map, INT};

/// Build the `world` argument passed to every event script function.
///
/// Fresh per call. Fields added here become accessible to modders without
/// touching any function signature. Adding a new field is the supported
/// extension mechanism.
pub fn build_world_view(
    world: &World,
    player: Entity,
    characters: &[Entity],
    choice_idx: usize,
) -> Map {
    let mut m = Map::new();
    m.insert("player".into(), character_view_from_world(world, player).into());
    let chars: Array = characters
        .iter()
        .map(|e| character_view_from_world(world, *e).into())
        .collect();
    m.insert("characters".into(), chars.into());
    m.insert("choice_idx".into(), Dynamic::from(choice_idx as INT));
    m
}

/// Substitute `{N.name}` placeholders in a template with the Nth character's
/// display name (0-indexed). Missing indices fall back to `"a stranger"`.
pub fn substitute_names(template: &str, characters: &[Map]) -> String {
    let mut result = template.to_string();
    for n in 0..=MAX_CHARACTER_INDEX {
        let placeholder = format!("{{{n}.name}}");
        let replacement = characters
            .get(n)
            .and_then(|m| m.get("name"))
            .and_then(|v| v.clone().into_string().ok())
            .unwrap_or_else(|| "a stranger".to_string());
        result = result.replace(&placeholder, &replacement);
    }
    result
}

/// Max indexed character the substitution helper handles. Modders using
/// more than this need to extend the constant — for the base game's three
/// events and most mods, 1-3 characters is the norm.
pub const MAX_CHARACTER_INDEX: usize = 9;

/// Attach a fresh `ScriptCtx` to a `world` map's `ctx` field, returning a
/// new map. Use this right before calling a script function that needs the
/// writeable API.
pub fn attach_ctx(world_map: Map, ctx: ScriptCtx) -> Map {
    let mut m = world_map;
    m.insert("ctx".into(), Dynamic::from(ctx));
    m
}

/// The single object passed to every event script function. Holds a raw
/// pointer to `World`; methods deref it. The wrapper is constructed per
/// call and dropped when the engine releases the `Dynamic`.
///
/// `Sync` is a lie we maintain by convention: at any moment there is at
/// most one `ScriptCtx` alive for a given world, and the engine never
/// touches the world while the wrapper is alive. Bevy itself enforces
/// exclusive `&mut World` access at a higher level.
#[derive(Clone, Copy)]
pub struct ScriptCtx {
    world_ptr: *mut World,
}

unsafe impl Send for ScriptCtx {}
unsafe impl Sync for ScriptCtx {}

impl ScriptCtx {
    /// Build a wrapper. Caller MUST drop the wrapper before any other code
    /// touches the world. The runtime helper `call_*` enforces this.
    pub fn new(world: &mut World) -> Self {
        Self {
            world_ptr: world as *mut World,
        }
    }

    /// Borrow the world mutably.
    fn world_mut(&mut self) -> &mut World {
        // Safety: caller (the runtime helper) guarantees exclusive access
        // for the wrapper's lifetime.
        unsafe { &mut *self.world_ptr }
    }

    /// Borrow the world immutably.
    fn world(&self) -> &World {
        // Safety: caller (the runtime helper) guarantees exclusive access
        // for the wrapper's lifetime.
        unsafe { &*self.world_ptr }
    }

    // ---- Read --------------------------------------------------------------

    /// Every alive character (excluding `self_actor`), as an array of
    /// character-view maps. Mirrors `commands::core::alive_characters_excluding`.
    pub fn alive_characters(&mut self, self_actor: Dynamic) -> Array {
        let actor_e = match dynamic_to_entity(self.world(), self_actor) {
            Some(e) => e,
            None => return Array::new(),
        };
        let chars = alive_characters_excluding(self.world_mut(), actor_e);
        chars
            .into_iter()
            .map(|(_, e)| character_view_from_world(self.world_mut(), e).into())
            .collect()
    }

    /// Every character (alive or dead), sorted by id.
    pub fn characters(&mut self) -> Array {
        let world = self.world_mut();
        let registry = world.resource::<Registry>();
        let mut out: Vec<(String, Entity)> = registry
            .by_id
            .iter()
            .filter(|(id, _)| id.starts_with("char-"))
            .map(|(id, e)| (id.clone(), *e))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter()
            .map(|(_, e)| character_view_from_world(world, e).into())
            .collect()
    }

    /// Look up a character by string id (`char-...`). Returns `()` if absent.
    pub fn character_by_id(&mut self, id: String) -> Dynamic {
        let world = self.world_mut();
        match world.resource::<Registry>().get(&id) {
            Some(e) => character_view_from_world(world, e).into(),
            None => Dynamic::UNIT,
        }
    }

    /// Today's in-game date — `(year, month, day)` tuple.
    pub fn date(&self) -> Array {
        let d = *self.world().resource::<Date>();
        vec![
            Dynamic::from(d.year as INT),
            Dynamic::from(d.month as INT),
            Dynamic::from(d.day as INT),
        ]
    }

    /// Whether the player is leading any kingdom currently at war.
    pub fn player_is_at_war(&self) -> bool {
        use crate::ecs::character::CharacterLeads;
        use crate::ecs::kingdom::KingdomHasWarsAttacking;
        let world = self.world();
        let player_e = match player_entity(world) {
            Some(e) => e,
            None => return false,
        };
        let leads = match world.get::<CharacterLeads>(player_e) {
            Some(cl) => cl,
            None => return false,
        };
        leads.kingdoms().iter().any(|k| {
            world
                .get::<KingdomHasWarsAttacking>(*k)
                .map(|w| !w.wars().is_empty())
                .unwrap_or(false)
        })
    }

    // ---- Write -------------------------------------------------------------

    /// Transfer gold from one character to another. Both args are character
    /// views (maps) or string ids. Fires `OnGoldGifted` so the chronicle
    /// observer writes a line.
    pub fn transfer_gold(&mut self, from: Dynamic, to: Dynamic, amount: INT) {
        let from_e = match dynamic_to_entity(self.world(), from) {
            Some(e) => e,
            None => return,
        };
        let to_e = match dynamic_to_entity(self.world(), to) {
            Some(e) => e,
            None => return,
        };
        if amount <= 0 {
            return;
        }
        let amount_i64: i64 = amount;
        let until = today_after_days(self.world_mut(), event_memory_days(amount_i64));
        transfer_with_gold_memory(self.world_mut(), from_e, to_e, amount_i64, until);
    }

    /// Spawn a memory entity: `recipient` remembers `toward` for `days`
    /// in-game days. `label` is reserved for future per-memory chronicle
    /// voicing.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_memory(&mut self, recipient: Dynamic, toward: Dynamic, label: String, days: INT) {
        use crate::ecs::character::{
            Memory, MemoryCreatedDate, MemoryKind, MemoryOfCharacter, MemoryTowardCharacter,
            MemoryUntilDate,
        };
        use crate::ecs::StringId;
        let recipient_e = match dynamic_to_entity(self.world(), recipient) {
            Some(e) => e,
            None => return,
        };
        let toward_e = match dynamic_to_entity(self.world(), toward) {
            Some(e) => e,
            None => return,
        };
        if days <= 0 {
            return;
        }
        let world = self.world_mut();
        let today = *world.resource::<Date>();
        let until = today_after_days(world, days as u32);
        let recipient_id = world
            .get::<StringId>(recipient_e)
            .map(|s| s.0.clone())
            .unwrap_or_else(|| format!("e{recipient_e:?}"));
        let toward_id = world
            .get::<StringId>(toward_e)
            .map(|s| s.0.clone())
            .unwrap_or_else(|| format!("e{toward_e:?}"));
        let memory_id = format!("memory-{recipient_id}-{toward_id}-{today}-script-{label}");
        let memory_e = world
            .spawn((
                StringId(memory_id.clone()),
                Memory,
                MemoryOfCharacter(recipient_e),
                MemoryTowardCharacter(toward_e),
                MemoryCreatedDate(today),
                MemoryUntilDate(until),
                MemoryKind::ReceivedGold { amount: 0 },
            ))
            .id();
        world
            .resource_mut::<Registry>()
            .by_id
            .insert(memory_id, memory_e);
    }

    /// Push a line to the chronicle log. Modders format the line themselves
    /// with the display names they read from `world.player.name`,
    /// `world.characters[N].name`, etc. No auto-substitution — keeps the
    /// runtime's substitution logic in one place (the popup / chronicle
    /// observer's `{N.name}` substitution).
    pub fn log(&mut self, line: String) {
        let world = self.world_mut();
        if !world.contains_resource::<Chronicles>() {
            return;
        }
        world.resource_mut::<Chronicles>().0.push(line);
    }

    /// RNG routed through the world's `SimRng` so replays stay deterministic.
    pub fn rng(&mut self, min: INT, max: INT) -> INT {
        if max < min {
            return min;
        }
        let span = (max - min + 1) as u32;
        let mut rng = self.world_mut().resource::<Game>().ctx.rng.lock().unwrap();
        let n = rand::TryRng::try_next_u32(&mut *rng).unwrap_or(0) % span;
        min + n as INT
    }
}

/// Register the `ScriptCtx` API on the engine. Called once in `main`.
pub fn register_api(engine: &mut Engine) {
    engine.register_type_with_name::<ScriptCtx>("ScriptCtx");
    // Read
    engine.register_fn("alive_characters", ScriptCtx::alive_characters);
    engine.register_fn("characters", ScriptCtx::characters);
    engine.register_fn("character_by_id", ScriptCtx::character_by_id);
    engine.register_fn("date", ScriptCtx::date);
    engine.register_fn("player_is_at_war", ScriptCtx::player_is_at_war);
    // Write
    engine.register_fn("transfer_gold", ScriptCtx::transfer_gold);
    engine.register_fn("grant_memory", ScriptCtx::grant_memory);
    engine.register_fn("log", ScriptCtx::log);
    // RNG — kept deterministic by routing through SimRng.
    engine.register_fn("rng", ScriptCtx::rng);
}

// ---- helpers ---------------------------------------------------------------

/// A character-view map — the shape modders see for `world.player`, each
/// element of `world.characters`, and each element of
/// `world.ctx.alive_characters()`.
pub fn character_view_from_world(world: &World, entity: Entity) -> Map {
    let mut m = Map::new();
    m.insert("entity".into(), Dynamic::from(entity.to_bits() as INT));
    let id = world
        .get::<crate::ecs::StringId>(entity)
        .map(|s| s.0.clone())
        .unwrap_or_default();
    let name = world
        .get::<CharacterName>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let house_id = world
        .get::<CharacterOfHouse>(entity)
        .and_then(|c| world.get::<crate::ecs::StringId>(c.0))
        .map(|s| s.0.clone())
        .unwrap_or_default();
    let levy = world
        .get::<CharacterLevy>(entity)
        .map(|l| l.0)
        .unwrap_or(0);
    let gold = world
        .get::<CharacterGold>(entity)
        .map(|g| g.0)
        .unwrap_or(0);
    let is_alive = world
        .get::<CharacterIsAlive>(entity)
        .map(|a| a.0)
        .unwrap_or(true);
    m.insert("id".into(), id.into());
    m.insert("name".into(), name.into());
    m.insert("house_id".into(), house_id.into());
    m.insert("levy".into(), Dynamic::from(levy as INT));
    m.insert("gold".into(), Dynamic::from(gold as INT));
    m.insert("is_alive".into(), is_alive.into());
    m
}

/// Resolve a script-side character arg to an `Entity`. Accepts a map
/// (character view with an `entity` int) or a string id (resolved through
/// the registry).
fn dynamic_to_entity(world: &World, d: Dynamic) -> Option<Entity> {
    if let Some(m) = d.clone().try_cast::<Map>() {
        if let Some(e) = m.get("entity").and_then(|v| v.as_int().ok()) {
            return Some(Entity::from_bits(e as u64));
        }
    }
    if let Ok(s) = d.clone().into_string() {
        return world.resource::<Registry>().get(&s);
    }
    None
}

fn player_entity(world: &World) -> Option<Entity> {
    world
        .resource::<Game>()
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| world.resource::<Registry>().get(id))
}

fn today_after_days(world: &World, days: u32) -> Date {
    let today = *world.resource::<Date>();
    let calendar = world.resource::<crate::resources::calendar::Calendar>().clone();
    today.after_days(days, &calendar)
}

fn event_memory_days(amount: i64) -> u32 {
    (amount as u32).saturating_mul(72)
}

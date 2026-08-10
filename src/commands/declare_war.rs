//! The declare-war command: declare war on another kingdom for a casus belli.
//!
//! Two selection steps: step 0 picks a defender kingdom (any kingdom other
//! than the actor's own), step 1 picks a casus belli type (only `Conquest`
//! exists today). The pick spawns a [`CasusBelli`](crate::ecs::CasusBelli)
//! entity targeting the defender, then a [`War`](crate::ecs::War) entity
//! linking the actor's kingdom (attacker) to the defender with that CB.
//!
//! The war has no resolution path yet — no tick, no army interaction, no
//! peace offering. The entity exists so the relationship graph is wired
//! (kingdom → wars → CB → target kingdom) and the chronicle records the
//! declaration. Resolution is a later change.
//!
//! The actor's kingdom is read through [`CharacterLeads`]; the kingdom
//! has no name field of its own, so the kingdom's display label is the
//! name of its held land (the convention everywhere else in the codebase
//! — a kingdom's seat is its single land).

use super::core::{Choice, Command, MenuItem, next_id, note};
use crate::ecs::{
    CasusBelli, CasusBelliKingdom, CasusBelliType, CharacterLeads, Kingdom, KingdomHold,
    LandName, Registry, StringId, War, WarAttackerKingdom, WarBeginDate, WarDefenderKingdom,
    WarName, WarWithCasusBelli,
};
use crate::resources::date::Date;
use bevy::ecs::world::World;

/// Declare war on a kingdom under a casus belli.
pub struct DeclareWar;

impl Command for DeclareWar {
    fn name(&self) -> &str {
        "Declare War"
    }

    fn step_count(&self) -> usize {
        2
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select a target kingdom",
            _ => "Select a casus belli",
        }
    }

    fn step_items(
        &self,
        step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        match step {
            0 => other_kingdoms(world, actor)
                .into_iter()
                .map(|(id, label)| MenuItem { label, value: id })
                .collect(),
            // Only one CB type exists today. Listed by stable id so a future
            // CB enum variant is additive — add a row, no other code changes.
            _ => vec![MenuItem {
                label: "Conquest (seize their land)".to_string(),
                value: "conquest".to_string(),
            }],
        }
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(defender_id) = choices.get(0).map(|c| c.value.as_str()) else {
            return;
        };
        let Some(cb_id) = choices.get(1).map(|c| c.value.as_str()) else {
            return;
        };
        declare(world, actor, defender_id, cb_id);
    }
}

/// `(kingdom_id, "<land_name>")` for every kingdom in the world except the
/// actor's own. Walks `World::iter_entities` (the `&World`-safe path — `query`
/// needs `&mut World`); filters by the [`Kingdom`] marker so we only see
/// kingdom entities.
fn other_kingdoms(world: &World, actor: &str) -> Vec<(String, String)> {
    let own_kingdom = world
        .resource::<Registry>()
        .get(actor)
        .and_then(|actor_e| world.get::<CharacterLeads>(actor_e))
        .map(|character_leads| character_leads.kingdom());

    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Kingdom>().is_none() {
            continue;
        }
        let kingdom_e = entity_ref.id();
        if Some(kingdom_e) == own_kingdom {
            continue;
        }
        let Some(string_id) = entity_ref.get::<StringId>() else {
            continue;
        };
        // The kingdom's display label is the name of its held land — a
        // kingdom has no name field of its own (its seat is its land).
        let label = entity_ref
            .get::<KingdomHold>()
            .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
            .map(|land_name| land_name.0.clone())
            .unwrap_or_else(|| string_id.0.clone());
        result.push((string_id.0.clone(), label));
    }
    result
}

/// Resolve the picked CB id to its [`CasusBelliType`]. Only `Conquest`
/// exists today; unknown ids are rejected. New CB enum variants are added
/// here (the menu row in `step_items` is the only other place).
fn resolve_cb(cb_id: &str) -> Option<CasusBelliType> {
    match cb_id {
        "conquest" => Some(CasusBelliType::Conquest),
        _ => None,
    }
}

/// Validate (actor rules a kingdom; defender is a different kingdom; CB id
/// resolves), then spawn a [`CasusBelli`] entity targeting the defender and
/// a [`War`] entity linking the actor's kingdom to the defender with that
/// CB. Appends a chronicle line on success and on every rejection.
fn declare(world: &mut World, actor: &str, defender_id: &str, cb_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, "cannot declare war: unknown actor".into());
    };
    let Some(attacker_kingdom_e) = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom())
    else {
        return note(world, "cannot declare war: you rule no kingdom".into());
    };
    let Some(defender_kingdom_e) = world.resource::<Registry>().get(defender_id) else {
        return note(
            world,
            format!("cannot declare war: no such kingdom `{defender_id}`"),
        );
    };
    if defender_kingdom_e == attacker_kingdom_e {
        return note(world, "cannot declare war on yourself".into());
    }
    let Some(cb_type) = resolve_cb(cb_id) else {
        return note(world, format!("unknown casus belli `{cb_id}`"));
    };

    // Capture display names before the spawn (cheap, immutable reads; gives
    // the chronicle line real names instead of bare ids).
    let attacker_name = kingdom_label(world, attacker_kingdom_e);
    let defender_name = kingdom_label(world, defender_kingdom_e);

    // Spawn the CB first; the war then references it. Both
    // `CasusBelliKingdom` and `WarAttackerKingdom`/`WarDefenderKingdom`/
    // `WarWithCasusBelli` are Bevy relationships, so the relationship hooks
    // fill the reverses (`KingdomHasCasusBelli`, `KingdomHasWarsAttacking`,
    // `KingdomHasWarsDefending`, `CasusBelliOnWar`) synchronously — any
    // same-frame reader sees authoritative data.
    let cb_entity_id = next_id(world);
    let cb_e = world
        .spawn((
            StringId(cb_entity_id.clone()),
            CasusBelli,
            cb_type,
            CasusBelliKingdom(defender_kingdom_e),
        ))
        .id();
    world.resource_mut::<Registry>().insert(cb_entity_id, cb_e);

    let war_entity_id = next_id(world);
    // Snapshot the date at declare time so the war carries a stable
    // "declared on" stamp that doesn't drift if the date resource ticks
    // over later. `format_name` reads the CB type + the defender's land
    // name; `Conquest` renders as `"Conquest over Kingdom of <land>"`.
    let begin_date = *world.resource::<Date>();
    let war_name = format_name(world, cb_type, defender_kingdom_e);
    let war_e = world
        .spawn((
            StringId(war_entity_id.clone()),
            War,
            WarAttackerKingdom(attacker_kingdom_e),
            WarDefenderKingdom(defender_kingdom_e),
            WarWithCasusBelli(cb_e),
            WarName(war_name),
            WarBeginDate(begin_date),
        ))
        .id();
    world.resource_mut::<Registry>().insert(war_entity_id, war_e);

    note(
        world,
        format!("{attacker_name} declared war on {defender_name} (conquest)"),
    );
}

/// Display label for a kingdom: the name of its held land, falling back to
/// the kingdom's string id. `KingdomHold` is read for the land entity; the
/// land's `LandName` gives the human-readable label.
fn kingdom_label(world: &World, kingdom_e: bevy::ecs::entity::Entity) -> String {
    world
        .get::<KingdomHold>(kingdom_e)
        .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| {
            world
                .get::<StringId>(kingdom_e)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| "?".into())
        })
}

/// Format a war's display name from the CB type + the defender kingdom's
/// held land. `Conquest` renders as `"Conquest over Kingdom of <land>"`.
/// New CB shapes are additive: one arm per variant here, one row in the
/// menu in `step_items`, one arm in `resolve_cb`.
fn format_name(
    world: &World,
    cb_type: CasusBelliType,
    defender_kingdom_e: bevy::ecs::entity::Entity,
) -> String {
    let land = kingdom_label(world, defender_kingdom_e);
    match cb_type {
        CasusBelliType::Conquest => format!("Conquest over Kingdom of {land}"),
    }
}

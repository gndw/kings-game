//! The gift-gold command: send some of the player's treasury to another
//! character. The recipient gains a memory of the gift that boosts their
//! opinion of the giver for as long as it lasts (see
//! [`crate::helper::opinion_helper`] for the formula).
//!
//! Three steps: command → target character → amount preset (10/25/50). The
//! amount step greys out any preset the player can't afford. A character
//! who already carries an active gold memory of anyone cannot be the target
//! of a fresh gift — the picker hides them, and `execute` re-checks in case
//! a memory materialised between picker render and execute.

use super::core::{
    error, picker_row, set_row_selected, transfer_with_gold_memory, BaseCommand, NAME_COLOR,
    STAT_COLOR, STAT_DIM,
};
use crate::app::Game;
use crate::ecs::character::{
    Character, CharacterIsAlive, CharacterName, Memory, MemoryKind, MemoryOfCharacter,
};
use crate::ecs::{KingdomGold, Registry, StringId};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

/// Preset amounts and the duration each one buys (in days). The formula is
/// `duration_days = amount × 72` — 10g grants +10 for 2y, 25g grants +25 for
/// 5y, 50g grants +50 for 10y, matching the docstring on
/// [`crate::helper::opinion_helper`].
const PRESETS: &[i64] = &[10, 25, 50];

pub struct GiftGold;

impl BaseCommand for GiftGold {
    fn get_command_id(&self) -> &'static str {
        "command:gift_gold"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        let command_pick = choices.iter().find(|(k, _)| k == "command").map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            let row = picker_row(
                world, parent, self.get_command_id(), None,
                "Gift Gold", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 2: pick a target character. Show every alive character except
        // the actor; skip those with an active gold memory.
        let target_pick = choices.iter().find(|(k, _)| k == "target_id").map(|(_, v)| v.clone());
        if target_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
            let rows = gift_targets(world, &actor);
            let mut entities = Vec::new();
            for (target_id, target_name, eligible) in rows {
                let (name, color) = if eligible {
                    (target_name, NAME_COLOR)
                } else {
                    (format!("{target_name} (recent gift)"), super::core::HINT_RED)
                };
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("target_id".to_string(), target_id)),
                    &name, color, None, None, None,
                );
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 3: pick an amount preset. Grey out presets the actor can't
        // afford (gold < amount) or that the target already disqualifies on.
        let amount_pick = choices.iter().find(|(k, _)| k == "amount").map(|(_, v)| v.clone());
        if amount_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
            let actor_gold = actor_gold(world, &actor);
            let mut entities = Vec::new();
            for &amount in PRESETS {
                let affordable = actor_gold >= amount;
                let (text, color) = if affordable {
                    (format!("{amount} gold"), STAT_COLOR)
                } else {
                    (format!("{amount} gold (need more)"), STAT_DIM)
                };
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("amount".to_string(), amount.to_string())),
                    &text, color, None, None, None,
                );
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 4: execute.
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let target_id = target_pick.as_deref().expect("step 2 reached without a target_id pick").to_string();
        let amount: i64 = amount_pick
            .as_deref()
            .expect("step 3 reached without an amount pick")
            .parse()
            .expect("amount pick should parse from a preset");
        gift(world, &actor, &target_id, amount);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

/// `(target_id, target_name, eligible)` for every alive character the player
/// could gift to. `eligible` is `false` when the character already carries an
/// active gold memory from anyone — gift stacking is off.
fn gift_targets(world: &mut World, actor: &str) -> Vec<(String, String, bool)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let actor_alive = world
        .get::<CharacterIsAlive>(actor_e)
        .map(|a| a.0)
        .unwrap_or(false);
    if !actor_alive {
        return Vec::new();
    }
    // Snapshot every active ReceivedGold memory's owner.
    let mut has_gold_memory: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    {
        let mut q = world.query_filtered::<(&MemoryOfCharacter, &MemoryKind), With<Memory>>();
        for (of, kind) in q.iter(world) {
            if matches!(kind, MemoryKind::ReceivedGold { .. }) {
                has_gold_memory.insert(of.0);
            }
        }
    }
    let registry = world.resource::<Registry>();
    let mut out: Vec<(String, String, bool)> = Vec::new();
    let mut entries: Vec<(String, Entity)> = registry
        .by_id
        .iter()
        .map(|(id, e)| (id.clone(), *e))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, e) in entries {
        if !id.starts_with("char-") {
            continue;
        }
        if e == actor_e {
            continue;
        }
        let Some(string_id) = world.get::<StringId>(e) else { continue };
        if string_id.0 != id {
            continue;
        }
        let Some(character) = world.get::<Character>(e) else { continue };
        let _ = character; // keep the query above as the gate
        let alive = world.get::<CharacterIsAlive>(e).map(|a| a.0).unwrap_or(false);
        if !alive {
            continue;
        }
        let name = world
            .get::<CharacterName>(e)
            .map(|n| n.0.clone())
            .unwrap_or_else(|| id.clone());
        let eligible = !has_gold_memory.contains(&e);
        out.push((id, name, eligible));
    }
    out
}

fn actor_gold(world: &World, actor: &str) -> i64 {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return 0;
    };
    primary_kingdom_gold(world, actor_e)
}

/// The actor's first ruled kingdom's gold, or 0 if they don't rule one.
fn primary_kingdom_gold(world: &World, character_e: Entity) -> i64 {
    get_character_ruled_kingdoms(world, character_e)
        .first()
        .and_then(|ke| world.get::<KingdomGold>(*ke))
        .map(|kg| kg.0)
        .unwrap_or(0)
}

/// Move `amount` gold from `actor`'s primary kingdom to a personal gift for
/// `target_id`, and spawn a memory on the recipient. The gold leaves the
/// realm's treasury — it doesn't credit the target's kingdom, even if they
/// rule one. A personal gift to a person is not a treasury transfer.
///
/// Validates actor's gold (their primary kingdom's) and target's eligibility.
fn gift(world: &mut World, actor: &str, target_id: &str, amount: i64) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot gift `{target_id}`: unknown actor"));
    };
    let Some(target_e) = world.resource::<Registry>().get(target_id) else {
        return error(world, format!("cannot gift `{target_id}`: no such character"));
    };
    if actor_e == target_e {
        return error(world, "cannot gift yourself".to_string());
    }

    // Phase 1 (no borrows held). Eligibility + actor gold check + snapshot.
    let actor_gold = primary_kingdom_gold(world, actor_e);
    if actor_gold < amount {
        return error(world, format!(
            "cannot gift `{target_id}`: need {amount} gold, have {actor_gold}"
        ));
    }
    // Re-check eligibility in case the target gained a gold memory between
    // picker render and execute (e.g. another player-driven event).
    {
        let mut q = world.query_filtered::<(&MemoryOfCharacter, &MemoryKind), With<Memory>>();
        let blocked = q.iter(world).any(|(of, kind)| {
            of.0 == target_e && matches!(kind, MemoryKind::ReceivedGold { .. })
        });
        if blocked {
            return error(world, format!(
                "cannot gift `{target_id}`: they already hold an active gift"
            ));
        }
    }
    // Snapshot calendar/date up front so the mutable-borrow phase doesn't have
    // to reach back into them.
    let until = {
        let calendar = world.resource::<Calendar>();
        let today = *world.resource::<Date>();
        today.after_days((amount as u32) * 72, calendar)
    };

    transfer_with_gold_memory(world, actor_e, target_e, amount, until);
}
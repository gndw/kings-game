//! The enforce-demands command: resolve one demand on a war the player is
//! fighting.
//!
//! Two selection steps: step 0 picks a war the player is attacking in
//! (from `KingdomHasWarsAttacking`), step 1 picks one of that war's
//! demands (`WarDemands` list). The pick resolves the demand:
//!
//! - **`WarDemandType::Take`** — only succeeds when the target kingdom's
//!   held land is controlled by one of the player's armies
//!   (`LandControlledByArmy` → army → `ArmyBelongsToKingdom` is the
//!   player's kingdom). On success, the target kingdom's `KingdomLedBy`
//!   is set to the player; Bevy's hook then auto-prunes the old
//!   `KingdomLedBy` and updates `CharacterLeads` on the new and old
//!   leaders.
//!
//! **Bevy's one-to-one leader rule applies.** `CharacterLeads` is a
//! single-`Entity` target on the character (one character leads at most
//! one kingdom), so the `Take` insert that gives the player the target
//! kingdom will drop the player's previous `KingdomLedBy` — the player's
//! old kingdom ends up leaderless. That's the literal "change target
//! kingdom leader to player" semantics; the multi-kingdom model is
//! future work.

use super::core::{Choice, Command, MenuItem, note};
use crate::ecs::{
    ArmyBelongsToKingdom, CharacterLeads, KingdomHasWarsAttacking, KingdomHold,
    LandControlledByArmy, LandName, Registry, StringId, WarDemands, WarName,
};
use crate::ecs::kingdom::KingdomLedBy;
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;

/// Resolve one demand on a player's war.
pub struct EnforceDemands;

impl Command for EnforceDemands {
    fn name(&self) -> &str {
        "Enforce Demands"
    }

    fn step_count(&self) -> usize {
        2
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select a war",
            _ => "Select a demand",
        }
    }

    fn step_items(
        &self,
        step: usize,
        choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        match step {
            0 => player_wars(world, actor)
                .into_iter()
                .map(|(id, label)| MenuItem { label, value: id })
                .collect(),
            _ => war_demands(world, choices),
        }
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(war_id) = choices.get(0).map(|c| c.value.as_str()) else {
            return;
        };
        let Some(demand_idx) = choices.get(1).map(|c| c.value.as_str()) else {
            return;
        };
        enforce(world, actor, war_id, demand_idx);
    }
}

/// `(war_id, "<WarName>")` for every war any of the player's kingdoms
/// is attacking in. Multi-kingdom: walks every kingdom the player leads
/// and unions their `KingdomHasWarsAttacking` lists, in
/// `CharacterLeads` order.
fn player_wars(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_has_wars) = world.get::<KingdomHasWarsAttacking>(*kingdom_e) else {
            continue;
        };
        for war_e in kingdom_has_wars.iter() {
            let Some(war_id) = world.get::<StringId>(war_e).map(|s| s.0.clone()) else {
                continue;
            };
            let war_name = world
                .get::<WarName>(war_e)
                .map(|war_name| war_name.0.clone())
                .unwrap_or_else(|| "?".into());
            out.push((war_id, war_name));
        }
    }
    out
}

/// One menu row per demand in the picked war's `WarDemands` list. The
/// `value` is the demand's index in the list (string-encoded), the
/// `label` is a human-readable shape like `"Take Kingdom of Riverrun"`.
/// Empty demands list → empty step-1 menu, so the palette shows nothing
/// and the player has to back out.
fn war_demands(world: &World, choices: &[Choice]) -> Vec<MenuItem> {
    let Some(war_id) = choices.get(0).map(|c| c.value.as_str()) else {
        return Vec::new();
    };
    let Some(war_e) = world.resource::<Registry>().get(war_id) else {
        return Vec::new();
    };
    let Some(w_demands) = world.get::<WarDemands>(war_e) else {
        return Vec::new();
    };
    w_demands
        .0
        .iter()
        .enumerate()
        .map(|(idx, demand)| {
            // Display label: shape + the target kingdom's land name (a
            // kingdom has no name field; its held land is its label).
            let target_label = world
                .get::<KingdomHold>(demand.target)
                .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let shape_label = match demand.demand_type {
                crate::ecs::WarDemandType::Take => "Take",
            };
            MenuItem {
                label: format!("{shape_label} Kingdom of {target_label}"),
                value: idx.to_string(),
            }
        })
        .collect()
}

/// Resolve the picked demand. `Take` only succeeds if the target
/// kingdom's held land is controlled by one of the player's armies —
/// then the kingdom's `KingdomLedBy` is set to the player.
fn enforce(world: &mut World, actor: &str, war_id: &str, demand_idx: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, "cannot enforce: unknown actor".into());
    };
    // Multi-kingdom: an army under any of the player's kingdoms counts
    // as "yours". The check in `enforce_take` walks the actor's kingdoms
    // and accepts a match on any of them — see there for the actual gate.
    // The `Some(_)` arm just validates the player leads at least one
    // kingdom (so we can refuse the empty case); `enforce_take` walks
    // them itself for the actual ownership check.
    let Some(_actor_kingdoms) = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>())
    else {
        return note(world, "cannot enforce: you rule no kingdom".into());
    };
    let Some(war_e) = world.resource::<Registry>().get(war_id) else {
        return note(world, format!("cannot enforce: no such war `{war_id}`"));
    };
    let Some(w_demands) = world.get::<WarDemands>(war_e) else {
        return note(world, format!("cannot enforce: war `{war_id}` has no demands"));
    };
    let Ok(idx) = demand_idx.parse::<usize>() else {
        return note(world, format!("cannot enforce: bad demand index `{demand_idx}`"));
    };
    let Some(demand) = w_demands.0.get(idx).copied() else {
        return note(world, format!("cannot enforce: demand `{idx}` out of range"));
    };

    // `Take` is the only demand shape today. Match on it; future shapes
    // are additive arms here. On a successful enforcement the war has
    // been resolved — despawn + deregister (mirroring `dismiss_army`'s
    // pattern). Bevy's relationship hooks on `WarAttackerKingdom` /
    // `WarDefenderKingdom` prune the war from both kingdoms'
    // `KingdomHasWarsAttacking` / `KingdomHasWarsDefending` collections
    // as part of the despawn — no manual reverse insert.
    if let Some(crate::ecs::WarDemandType::Take) =
        enforce_take(world, actor_e, demand.target)
    {
        world.despawn(war_e);
        world.resource_mut::<Registry>().by_id.remove(war_id);
        note(world, format!("war `{war_id}` resolved and ended"));
    }
}

/// `Take` — flip the target kingdom's leader to the player. Gate: the
/// target's held land must already be controlled by an army belonging
/// to the player's kingdom (the conquest transfer follows the
/// siege-then-control flow; the demand just enforces the legal transfer
/// step). On success, Bevy's hook on `KingdomLedBy` prunes the old
/// leader's `CharacterLeads` and the player's `CharacterLeads` is
/// adds the entry to the player's `CharacterLeads` `Vec` (multi-kingdom)
/// and prunes the old leader's `CharacterLeads` entry.
fn enforce_take(
    world: &mut World,
    actor_e: bevy::ecs::entity::Entity,
    target_kingdom_e: bevy::ecs::entity::Entity,
) -> Option<crate::ecs::WarDemandType> {
    // Gate: the target kingdom's held land must be controlled by one
    // of the player's armies. Multi-kingdom: any of the actor's
    // kingdoms owning the controlling army counts.
    let target_land = world
        .get::<KingdomHold>(target_kingdom_e)
        .map(|kingdom_hold| kingdom_hold.0);
    let Some(target_land) = target_land else {
        note(
            world,
            "cannot enforce Take: target kingdom has no land".into(),
        );
        return None;
    };
    let Some(controlling_army) = world
        .get::<LandControlledByArmy>(target_land)
        .map(|land_controlled_by_army| land_controlled_by_army.army())
    else {
        note(
            world,
            "cannot enforce Take: target land is not controlled by your army".into(),
        );
        return None;
    };
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(controlling_army)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    if !actor_kingdoms.contains(&army_kingdom.unwrap_or(bevy::ecs::entity::Entity::PLACEHOLDER)) {
        note(
            world,
            "cannot enforce Take: target land is not controlled by your army".into(),
        );
        return None;
    }

    // Capture labels before mutating (cheap, immutable reads).
    let target_name = world
        .get::<LandName>(target_land)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());

    // Insert the new `KingdomLedBy`. Bevy's relationship hook updates
    // `CharacterLeads` on the new leader (player) — under the
    // multi-kingdom model the player can lead the new kingdom AND keep
    // any kingdoms they already led; Bevy adds to the player's
    // `CharacterLeads` Vec instead of replacing (the old leader, if
    // any, has the entry pruned).
    world.entity_mut(target_kingdom_e).insert(KingdomLedBy(actor_e));

    note(
        world,
        format!(
            "took the Kingdom of {target_name} (Take enforced)"
        ),
    );
    Some(crate::ecs::WarDemandType::Take)
}

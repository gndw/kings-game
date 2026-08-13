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

use super::core::{error, note, picker_row, set_row_selected, BaseCommand, NAME_COLOR, STAT_COLOR,
    STAT_DIM};
use crate::ecs::{
    ArmyBelongsToKingdom, CharacterLeads, KingdomHasWarsAttacking, KingdomHold,
    LandControlledByArmy, LandName, Registry, WarBeginDate, WarDefenderKingdom, WarDemandType,
    WarDemands, WarName,
};
use crate::ecs::kingdom::KingdomLedBy;
use crate::app::Game;
use crate::ui::command_menu::CommandMenuUiContext;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Resolve one demand on a player's war.
pub struct EnforceDemands;

impl BaseCommand for EnforceDemands {
    fn get_command_id(&self) -> &'static str {
        "command:enforce_demands"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            return self.spawn_command_row(world, parent);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: pick the war.
        let war_pick = choices
            .iter()
            .find(|(k, _)| k == "war_id")
            .map(|(_, v)| v.clone());
        if war_pick.is_none() {
            return self.spawn_war_picker(world, parent);
        }

        // Step 2: pick the demand.
        let demand_pick = choices
            .iter()
            .find(|(k, _)| k == "demand_idx")
            .map(|(_, v)| v.clone());
        if demand_pick.is_none() {
            return self.spawn_demand_picker(world, parent, &war_pick.unwrap());
        }

        // Execute.
        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl EnforceDemands {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = picker_row(
            world,
            parent,
            self.get_command_id(),
            None,
            "Enforce Demands",
            NAME_COLOR,
            None,
            None,
            None,
        );
        (vec![row], false)
    }

    fn spawn_war_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let calendar = world.resource::<Calendar>();
        let date = world.resource::<Date>();
        let rows = player_war_rows(world, &actor, calendar, date);
        let mut entities = Vec::new();
        for row_data in rows {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("war_id".to_string(), row_data.war_id)),
                &row_data.name,
                NAME_COLOR,
                row_data.description.as_deref(),
                Some((row_data.age.as_str(), STAT_COLOR)),
                Some((row_data.demands_left.as_str(), STAT_DIM)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_demand_picker(
        &self,
        world: &mut World,
        parent: Entity,
        war_id: &str,
    ) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let demands = demand_rows(world, &actor, war_id);
        let mut entities = Vec::new();
        for row_data in demands {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("demand_idx".to_string(), row_data.idx)),
                &row_data.name,
                row_data.name_color,
                None,
                None,
                Some((row_data.gate.as_str(), row_data.gate_color)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let picks: Vec<(String, String)> =
            world.resource::<CommandMenuUiContext>().choices.clone();
        let war_id = picks
            .iter()
            .find(|(k, _)| k == "war_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a war_id pick");
        let demand_idx = picks
            .iter()
            .find(|(k, _)| k == "demand_idx")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a demand_idx pick");
        enforce(world, &actor, &war_id, &demand_idx);
        (Vec::new(), true)
    }
}

/// One war's picker row data. `age` is the time since `WarBeginDate`
/// (formatted via `Calendar::format_duration`); `demands_left` is the
/// remaining count in `WarDemands`.
struct WarRowData {
    war_id: String,
    name: String,
    description: Option<String>,
    age: String,
    demands_left: String,
}

/// Walk every kingdom the actor leads and union their
/// `KingdomHasWarsAttacking` lists, in `CharacterLeads` order.
fn player_war_rows(
    world: &World,
    actor: &str,
    calendar: &Calendar,
    date: &Date,
) -> Vec<WarRowData> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(khwa) = world.get::<KingdomHasWarsAttacking>(*kingdom_e) else {
            continue;
        };
        for war_e in khwa.iter() {
            let Some(war_id) = world.get::<crate::ecs::StringId>(war_e).map(|s| s.0.clone()) else {
                continue;
            };
            let war_name = world
                .get::<WarName>(war_e)
                .map(|wn| wn.0.clone())
                .unwrap_or_else(|| "?".into());
            // War age: today − `WarBeginDate`, formatted. Missing
            // begin date → empty.
            let age = world
                .get::<WarBeginDate>(war_e)
                .map(|WarBeginDate(begin)| {
                    let dur = (date.ordinal(calendar) - begin.ordinal(calendar)).max(0) as u32;
                    calendar.format_duration(dur)
                })
                .unwrap_or_default();
            let demands_left = world
                .get::<WarDemands>(war_e)
                .map(|wd| wd.0.len().to_string())
                .unwrap_or_default();
            // Description: defender's kingdom label.
            let defender_name = world
                .get::<WarDefenderKingdom>(war_e)
                .and_then(|war_defender_kingdom| {
                    world
                        .get::<KingdomHold>(war_defender_kingdom.0)
                        .and_then(|kh| world.get::<LandName>(kh.0))
                })
                .map(|ln| ln.0.clone())
                .unwrap_or_default();
            out.push(WarRowData {
                war_id,
                name: war_name,
                description: if defender_name.is_empty() {
                    None
                } else {
                    Some(format!("vs {defender_name}"))
                },
                age,
                demands_left: if demands_left.is_empty() {
                    String::new()
                } else {
                    format!("{demands_left} left")
                },
            });
        }
    }
    out
}

/// One demand's picker row data. `gate` is `ready` / `block: hold <land>`
/// depending on whether the `Take` demand's gate (target land controlled
/// by a player's army) is currently met; unmet demands get a red name
/// tint + `(blocked)` suffix so the player sees the pick will fail.
struct DemandRowData {
    idx: String,
    name: String,
    name_color: Color,
    gate: String,
    gate_color: Color,
}

/// Resolve each demand on the picked war into picker-row data. `Take` is
/// the only shape today; the gate is `ready` if the target kingdom's
/// held land is controlled by one of the actor's armies, otherwise
/// `block: hold <land>`.
fn demand_rows(world: &World, actor: &str, war_id: &str) -> Vec<DemandRowData> {
    let Some(war_e) = world.resource::<Registry>().get(war_id) else {
        return Vec::new();
    };
    let Some(wd) = world.get::<WarDemands>(war_e) else {
        return Vec::new();
    };
    let actor_kingdoms: std::collections::HashSet<bevy::ecs::entity::Entity> = world
        .resource::<Registry>()
        .get(actor)
        .and_then(|actor_e| world.get::<CharacterLeads>(actor_e))
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for (idx, demand) in wd.0.iter().enumerate() {
        let shape = match demand.demand_type {
            WarDemandType::Take => "Take",
        };
        let target_label = world
            .get::<KingdomHold>(demand.target)
            .and_then(|kh| world.get::<LandName>(kh.0))
            .map(|ln| ln.0.clone())
            .unwrap_or_else(|| "?".into());
        let (gate, met) = match demand.demand_type {
            WarDemandType::Take => {
                // Gate: target land must be controlled by one of the
                // actor's armies (any of their kingdoms).
                let target_land = world
                    .get::<KingdomHold>(demand.target)
                    .map(|kh| kh.0);
                let controlling_army = target_land.and_then(|l| world.get::<LandControlledByArmy>(l));
                let army_ok = controlling_army
                    .and_then(|lca| world.get::<ArmyBelongsToKingdom>(lca.army()))
                    .map(|abtk| actor_kingdoms.contains(&abtk.0))
                    .unwrap_or(false);
                if army_ok {
                    ("ready".to_string(), true)
                } else {
                    (format!("block: hold {target_label}"), false)
                }
            }
        };
        let (name, name_color) = if met {
            (format!("{shape} Kingdom of {target_label}"), NAME_COLOR)
        } else {
            (
                format!("{shape} Kingdom of {target_label} (blocked)"),
                super::core::HINT_RED,
            )
        };
        let gate_color = if met { STAT_COLOR } else { STAT_DIM };
        out.push(DemandRowData {
            idx: idx.to_string(),
            name,
            name_color,
            gate,
            gate_color,
        });
    }
    out
}

/// Resolve the picked demand. `Take` only succeeds if the target
/// kingdom's held land is controlled by one of the player's armies —
/// then the kingdom's `KingdomLedBy` is set to the player.
fn enforce(world: &mut World, actor: &str, war_id: &str, demand_idx: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, "cannot enforce: unknown actor".into());
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
        return error(world, "cannot enforce: you rule no kingdom".into());
    };
    let Some(war_e) = world.resource::<Registry>().get(war_id) else {
        return error(world, format!("cannot enforce: no such war `{war_id}`"));
    };
    let Some(w_demands) = world.get::<WarDemands>(war_e) else {
        return error(world, format!("cannot enforce: war `{war_id}` has no demands"));
    };
    let Ok(idx) = demand_idx.parse::<usize>() else {
        return error(world, format!("cannot enforce: bad demand index `{demand_idx}`"));
    };
    let Some(demand) = w_demands.0.get(idx).copied() else {
        return error(world, format!("cannot enforce: demand `{idx}` out of range"));
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
        error(
            world,
            "cannot enforce Take: target kingdom has no land".into(),
        );
        return None;
    };
    let Some(controlling_army) = world
        .get::<LandControlledByArmy>(target_land)
        .map(|land_controlled_by_army| land_controlled_by_army.army())
    else {
        error(
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
        error(
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

//! The enforce-demands command: resolve one demand on a war the player is fighting.
//!
//! Two steps: pick a war (from `KingdomHasWarsAttacking`), pick a demand.
//! `Take` only succeeds when the target kingdom's held land is controlled by one
//! of the player's armies; on success the target kingdom's Ruler courtier is
//! swapped to the player via [`set_ruler`].

use super::core::{error, picker_row, set_row_selected, BaseCommand, NAME_COLOR, STAT_COLOR,
    STAT_DIM};
use crate::ecs::{
    ArmyBelongsToKingdom, KingdomHasWarsAttacking, KingdomHold,
    LandControlledByArmy, LandName, Registry, WarBeginDate, WarDefenderKingdom, WarDemandType,
    WarDemands, WarName,
};
use crate::helper::kingdom_helper::{character_ruled_kingdoms, set_ruler};
use crate::observers::{OnDemandEnforced, OnWarEnded};
use crate::app::Game;
use crate::ui::command_menu::CommandMenuUiContext;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

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
        let command_pick = choices.iter().find(|(k, _)| k == "command").map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            return self.spawn_command_row(world, parent);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let war_pick = choices.iter().find(|(k, _)| k == "war_id").map(|(_, v)| v.clone());
        if war_pick.is_none() {
            return self.spawn_war_picker(world, parent);
        }

        let demand_pick = choices.iter().find(|(k, _)| k == "demand_idx").map(|(_, v)| v.clone());
        if demand_pick.is_none() {
            return self.spawn_demand_picker(world, parent, &war_pick.unwrap());
        }

        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl EnforceDemands {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = picker_row(
            world, parent, self.get_command_id(), None,
            "Enforce Demands", NAME_COLOR, None, None, None,
        );
        (vec![row], false)
    }

    fn spawn_war_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let calendar = world.resource::<Calendar>();
        let date = world.resource::<Date>();
        let rows = player_war_rows(world, &actor, calendar, date);
        let mut entities = Vec::new();
        for row_data in rows {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("war_id".to_string(), row_data.war_id)),
                &row_data.name, NAME_COLOR,
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
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let demands = demand_rows(world, &actor, war_id);
        let mut entities = Vec::new();
        for row_data in demands {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("demand_idx".to_string(), row_data.idx)),
                &row_data.name, row_data.name_color, None,
                None,
                Some((row_data.gate.as_str(), row_data.gate_color)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let picks: Vec<(String, String)> = world.resource::<CommandMenuUiContext>().choices.clone();
        let war_id = picks.iter().find(|(k, _)| k == "war_id").map(|(_, v)| v.clone())
            .expect("execute reached without a war_id pick");
        let demand_idx = picks.iter().find(|(k, _)| k == "demand_idx").map(|(_, v)| v.clone())
            .expect("execute reached without a demand_idx pick");
        enforce(world, &actor, &war_id, &demand_idx);
        (Vec::new(), true)
    }
}

struct WarRowData {
    war_id: String,
    name: String,
    description: Option<String>,
    age: String,
    demands_left: String,
}

/// Walk every kingdom the actor leads and union their `KingdomHasWarsAttacking` lists.
fn player_war_rows(
    world: &World,
    actor: &str,
    calendar: &Calendar,
    date: &Date,
) -> Vec<WarRowData> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_ruled_kingdoms(world, actor_e) {
        let Some(khwa) = world.get::<KingdomHasWarsAttacking>(kingdom_e) else {
            continue;
        };
        for war_e in khwa.iter() {
            let Some(war_id) = world.get::<crate::ecs::StringId>(war_e).map(|s| s.0.clone()) else {
                continue;
            };
            let war_name = world.get::<WarName>(war_e).map(|wn| wn.0.clone()).unwrap_or_else(|| "?".into());
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
            let defender_name = world
                .get::<WarDefenderKingdom>(war_e)
                .and_then(|war_defender_kingdom| {
                    world.get::<KingdomHold>(war_defender_kingdom.0)
                        .and_then(|kh| world.get::<LandName>(kh.0))
                })
                .map(|ln| ln.0.clone())
                .unwrap_or_default();
            out.push(WarRowData {
                war_id,
                name: war_name,
                description: if defender_name.is_empty() { None } else { Some(format!("vs {defender_name}")) },
                age,
                demands_left: if demands_left.is_empty() { String::new() } else { format!("{demands_left} left") },
            });
        }
    }
    out
}

struct DemandRowData {
    idx: String,
    name: String,
    name_color: Color,
    gate: String,
    gate_color: Color,
}

/// Resolve each demand on the picked war into picker-row data.
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
        .map(|actor_e| character_ruled_kingdoms(world, actor_e).into_iter().collect())
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
                let target_land = world.get::<KingdomHold>(demand.target).map(|kh| kh.0);
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
            (format!("{shape} Kingdom of {target_label} (blocked)"), super::core::HINT_RED)
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

/// Resolve the picked demand. `Take` only succeeds if the target kingdom's held land is controlled by one of the player's armies.
fn enforce(world: &mut World, actor: &str, war_id: &str, demand_idx: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, "cannot enforce: unknown actor".into());
    };
    let actor_kingdoms = character_ruled_kingdoms(world, actor_e);
    if actor_kingdoms.is_empty() {
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

    if let Some(crate::ecs::WarDemandType::Take) = enforce_take(world, actor_e, demand.target) {
        let defender = world.get::<WarDefenderKingdom>(war_e).map(|w| w.0);
        world.despawn(war_e);
        world.resource_mut::<Registry>().by_id.remove(war_id);
        if let Some(defender_kingdom) = defender {
            world.trigger(OnWarEnded { defender: defender_kingdom });
        }
    }
}

/// `Take` — flip the target kingdom's leader to the player. Gate: the target's
/// held land must be controlled by an army under one of the player's kingdoms.
fn enforce_take(
    world: &mut World,
    actor_e: bevy::ecs::entity::Entity,
    target_kingdom_e: bevy::ecs::entity::Entity,
) -> Option<crate::ecs::WarDemandType> {
    let target_land = world.get::<KingdomHold>(target_kingdom_e).map(|kh| kh.0);
    let Some(target_land) = target_land else {
        error(world, "cannot enforce Take: target kingdom has no land".into());
        return None;
    };
    let Some(controlling_army) = world
        .get::<LandControlledByArmy>(target_land)
        .map(|land_controlled_by_army| land_controlled_by_army.army())
    else {
        error(world, "cannot enforce Take: target land is not controlled by your army".into());
        return None;
    };
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(controlling_army)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let actor_kingdoms = character_ruled_kingdoms(world, actor_e);
    if !actor_kingdoms.contains(&army_kingdom.unwrap_or(bevy::ecs::entity::Entity::PLACEHOLDER)) {
        error(world, "cannot enforce Take: target land is not controlled by your army".into());
        return None;
    }

    // Swap the Ruler first so the observers triggered below (`building_releasing`)
    // see the new leader. `court_releasing` skips `type: Ruler` courtiers so the
    // freshly-spawned one survives the sweep.
    set_ruler(world, target_kingdom_e, Some(actor_e));

    world.trigger(OnDemandEnforced {
        demand_type: crate::ecs::WarDemandType::Take,
        target: target_kingdom_e,
    });

    Some(crate::ecs::WarDemandType::Take)
}

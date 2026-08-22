//! The marching-army command: queue marching orders to move one of the actor's
//! armies to another land. Armies travel by road, one marching per road.
//!
//! Two steps: pick an army (any army under the actor's kingdom), pick a target
//! land. The daily `game::marching::on_day` does the actual lifting.

use super::core::{
    army_status_text, error, next_id, picker_row, set_row_selected, BaseCommand, NAME_COLOR,
    STAT_COLOR, STAT_DIM,
};
use crate::ecs::army::{
    ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand,
};
use crate::ecs::character::CharacterName;
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::KingdomHasArmies;
use crate::ecs::marching::{
    Marching, MarchingArmy, MarchingArrivedDate, MarchingBeginDate, MarchingFromLand,
    MarchingOnRoad, MarchingStatus, MarchingToLand,
};
use crate::ecs::road::{Road, RoadBetweenLands};
use crate::ecs::{CharacterOfHouse, Land, LandHeldBy, LandName, Registry, StringId};
use crate::helper::kingdom_helper::{character_ruled_kingdoms, kingdom_ruler};
use crate::observers::OnMarchingOrdered;
use crate::ui::command_menu::CommandMenuUiContext;
use crate::app::Game;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::game::marching::road_days;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;
use std::collections::HashMap;
use std::collections::VecDeque;

/// Queue a marching order for one of the actor's armies. Registered as "Marching Army".
pub struct MarchingOrder;

impl BaseCommand for MarchingOrder {
    fn get_command_id(&self) -> &'static str {
        "command:marching_order"
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
            let row = picker_row(
                world, parent, self.get_command_id(), None,
                "Marching Army", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let army_pick = choices.iter().find(|(k, _)| k == "army_id").map(|(_, v)| v.clone());
        if army_pick.is_none() {
            return self.spawn_army_picker(world, parent);
        }

        let target_pick = choices.iter().find(|(k, _)| k == "target_id").map(|(_, v)| v.clone());
        if target_pick.is_none() {
            return self.spawn_target_picker(world, parent, army_pick.as_deref().unwrap());
        }

        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl MarchingOrder {
    fn spawn_army_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let armies = armies_under(world, &actor);
        let mut entities = Vec::new();
        for (army_id, name, current_land, levy, status) in armies {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("army_id".to_string(), army_id)),
                &name, NAME_COLOR,
                Some(&format!("at {current_land}")),
                Some((&levy.to_string(), STAT_COLOR)),
                status.as_deref().map(|s| (s, STAT_COLOR)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_target_picker(
        &self,
        world: &mut World,
        parent: Entity,
        army_id: &str,
    ) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let own_kingdoms: std::collections::HashSet<Entity> = world
            .resource::<Registry>()
            .get(&actor)
            .map(|actor_e| character_ruled_kingdoms(world, actor_e).into_iter().collect())
            .unwrap_or_default();
        let army_land_e = world
            .resource::<Registry>()
            .get(army_id)
            .and_then(|army_e| world.get::<ArmyOnLand>(army_e))
            .map(|aol| aol.0);
        let calendar = world.resource::<Calendar>();
        let rows = all_lands_target_rows(world, army_land_e, &own_kingdoms, calendar);
        let mut entities = Vec::new();
        for row_data in rows {
            let row = picker_row(
                world, parent, self.get_command_id(),
                Some(("target_id".to_string(), row_data.land_id)),
                &row_data.name, row_data.name_color,
                row_data.description.as_deref(),
                Some((row_data.days.as_str(), row_data.days_color)),
                None,
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let picks: Vec<(String, String)> = world.resource::<CommandMenuUiContext>().choices.clone();
        let army_id = picks.iter().find(|(k, _)| k == "army_id").map(|(_, v)| v.clone())
            .expect("execute reached without an army_id pick");
        let target_id = picks.iter().find(|(k, _)| k == "target_id").map(|(_, v)| v.clone())
            .expect("execute reached without a target_id pick");
        march(world, &actor, &army_id, &target_id);
        (Vec::new(), true)
    }
}

/// `(army_id, name, current_land, levy, status_text)` for every army the actor rules.
fn armies_under(
    world: &World,
    actor: &str,
) -> Vec<(String, String, String, u64, Option<String>)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let calendar = world.resource::<Calendar>();
    let date = world.resource::<Date>();
    let mut out = Vec::new();
    for kingdom_e in character_ruled_kingdoms(world, actor_e) {
        let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(kingdom_e) else {
            continue;
        };
        for army_e in kingdom_has_armies.iter() {
            let Some(string_id) = world.get::<StringId>(army_e) else { continue };
            let Some(name) = world.get::<ArmyName>(army_e) else { continue };
            let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else { continue };
            let current_land = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world.get::<ArmyLevy>(army_e).map(|army_levy| army_levy.0).unwrap_or(0);
            let status = army_status_text(world, army_e, calendar, date);
            out.push((string_id.0.clone(), name.0.clone(), current_land, levy, status));
        }
    }
    out
}

/// One land's target-picker row data.
struct TargetRowData {
    land_id: String,
    name: String,
    name_color: Color,
    description: Option<String>,
    days: String,
    days_color: Color,
}

/// Walk every land and assemble its target-picker row.
fn all_lands_target_rows(
    world: &World,
    army_land_e: Option<Entity>,
    own_kingdoms: &std::collections::HashSet<Entity>,
    calendar: &Calendar,
) -> Vec<TargetRowData> {
    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Land>().is_none() {
            continue;
        }
        let land_e = entity_ref.id();
        let Some(sid) = entity_ref.get::<StringId>() else { continue };
        let Some(land_name) = entity_ref.get::<LandName>() else { continue };

        let (ruler_text, _ruler_color) = ruler_text(world, entity_ref.get::<LandHeldBy>());

        let is_own = entity_ref
            .get::<LandHeldBy>()
            .map(|land_held_by| own_kingdoms.contains(&land_held_by.kingdom()))
            .unwrap_or(false);

        let days_text = match (army_land_e, Some(land_e)) {
            (Some(from), Some(to)) if from != to => match trace(world, from, to) {
                Some(hops) => {
                    let total: u32 = hops.iter().filter_map(|h| road_days(world, h.road)).sum();
                    calendar.format_duration(total)
                }
                None => "—".into(),
            },
            (Some(from), Some(to)) if from == to => "0d".into(),
            _ => "—".into(),
        };

        let is_home = is_own && army_land_e == Some(land_e);
        let (name, name_color, days_color) = match (is_home, army_land_e, days_text == "—") {
            (true, _, _) => (
                format!("{} (home)", land_name.0),
                super::core::HINT_RED,
                STAT_DIM,
            ),
            (false, Some(from), true) if from != land_e => (
                format!("{} (no route)", land_name.0),
                super::core::HINT_RED,
                STAT_DIM,
            ),
            _ => (land_name.0.clone(), NAME_COLOR, STAT_COLOR),
        };
        let description = if ruler_text.is_empty() { None } else { Some(ruler_text.clone()) };

        result.push(TargetRowData {
            land_id: sid.0.clone(),
            name,
            name_color,
            description,
            days: days_text,
            days_color,
        });
    }
    result
}

/// One-line ruler label: `"<character>, <house>"` or empty when missing.
fn ruler_text(world: &World, land_held_by: Option<&LandHeldBy>) -> (String, Color) {
    let Some(kingdom_e) = land_held_by.map(|land_held_by| land_held_by.kingdom()) else {
        return (String::new(), STAT_DIM);
    };
    let Some(leader_e) = kingdom_ruler(world, kingdom_e) else {
        return (String::new(), STAT_DIM);
    };
    let Some(character_name) = world.get::<CharacterName>(leader_e) else {
        return (String::new(), STAT_DIM);
    };
    let house_name = world
        .get::<CharacterOfHouse>(leader_e)
        .and_then(|coh| world.get::<HouseName>(coh.0))
        .map(|hn| hn.0.clone());
    let text = match house_name {
        Some(h) => format!("{}, {}", character_name.0, h),
        None => character_name.0.clone(),
    };
    (text, STAT_COLOR)
}

/// One road on a traced route: the road entity and the land at each end.
struct Hop {
    road: Entity,
    from: Entity,
    to: Entity,
}

/// The road adjacency: `land → [(road, other end)]`. Roads are baked at populate time and never change.
fn road_graph(world: &World) -> HashMap<Entity, Vec<(Entity, Entity)>> {
    let mut graph: HashMap<Entity, Vec<(Entity, Entity)>> = HashMap::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Road>().is_none() {
            continue;
        }
        let Some(between) = entity_ref.get::<RoadBetweenLands>() else { continue };
        let [a, b] = between.0[..] else { continue };
        let road_e = entity_ref.id();
        graph.entry(a).or_default().push((road_e, b));
        graph.entry(b).or_default().push((road_e, a));
    }
    graph
}

/// BFS from `from_e` to `to_e`, returning one `Hop` per road in travel order.
fn trace(world: &World, from_e: Entity, to_e: Entity) -> Option<Vec<Hop>> {
    let graph = road_graph(world);

    let mut came_from: HashMap<Entity, (Entity, Entity)> = HashMap::new();
    let mut queue: VecDeque<Entity> = VecDeque::from([from_e]);
    while let Some(land_e) = queue.pop_front() {
        if land_e == to_e {
            break;
        }
        for &(road_e, next_e) in graph.get(&land_e).into_iter().flatten() {
            if next_e == from_e || came_from.contains_key(&next_e) {
                continue;
            }
            came_from.insert(next_e, (road_e, land_e));
            queue.push_back(next_e);
        }
    }

    let mut hops = Vec::new();
    let mut cursor = to_e;
    while cursor != from_e {
        let (road_e, prev_e) = *came_from.get(&cursor)?;
        hops.push(Hop { road: road_e, from: prev_e, to: cursor });
        cursor = prev_e;
    }
    hops.reverse();
    Some(hops)
}

/// Validate, then spawn one marching entity per road on the route — each `Scheduled` with empty dates.
fn march(world: &mut World, actor: &str, army_id: &str, target_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot march `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return error(world, format!("cannot march `{army_id}`: no such army"));
    };
    let Some(target_e) = world.resource::<Registry>().get(target_id) else {
        return error(world, format!("cannot march to `{target_id}`: no such land"));
    };

    let actor_kingdoms = character_ruled_kingdoms(world, actor_e);
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let _ = match (actor_kingdoms, army_kingdom) {
        (aks, Some(ak)) if aks.contains(&ak) => ak,
        _ => {
            return error(
                world,
                format!(
                    "cannot march `{army_id}`: that army does not belong to your kingdom"
                ),
            );
        }
    };

    let from_e = world.get::<ArmyOnLand>(army_e).map(|army_on_land| army_on_land.0);
    let Some(from_e) = from_e else {
        return error(world, format!("cannot march `{army_id}`: army has no land"));
    };
    if from_e == target_e {
        return error(world, format!(
            "cannot march `{army_id}`: the army is already on {target_id}"
        ));
    }

    let from_name = world.get::<LandName>(from_e).map(|land_name| land_name.0.clone()).unwrap_or_else(|| "?".into());
    let to_name = world.get::<LandName>(target_e).map(|land_name| land_name.0.clone()).unwrap_or_else(|| target_id.to_string());

    let Some(hops) = trace(world, from_e, target_e) else {
        return error(world, format!(
            "cannot march `{army_id}`: no road leads from {from_name} to {to_name}"
        ));
    };

    let Some(days) = hops.iter().map(|hop| road_days(world, hop.road)).sum::<Option<u32>>() else {
        return error(world, format!(
            "cannot march `{army_id}`: a road on the way to {to_name} has no distance"
        ));
    };

    for hop in &hops {
        let id = next_id(world);
        let eid = world
            .spawn((
                StringId(id.clone()),
                Marching,
                MarchingArmy(army_e),
                MarchingFromLand(hop.from),
                MarchingToLand(hop.to),
                MarchingOnRoad(hop.road),
                MarchingStatus::Scheduled,
                MarchingBeginDate(None),
                MarchingArrivedDate(None),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    let roads = hops.len() as u32;

    world.trigger(OnMarchingOrdered {
        army: army_e,
        from: from_e,
        to: target_e,
        roads,
        days,
    });
}

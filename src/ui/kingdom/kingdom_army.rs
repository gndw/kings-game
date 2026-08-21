//! Kingdom army rows — one line per army, with current land + status
//! (idle / raising / marching route / sieging progress).

use crate::ecs::army::{
    ArmyHasMarching, ArmyHasSiege, ArmyLevy, ArmyMarching, ArmyMaxLevy, ArmyName, ArmyOnLand,
    ArmyStatus,
};
use crate::ecs::kingdom::KingdomHasArmies;
use crate::ecs::land::LandName;
use crate::ecs::marching::{
    MarchingArrivedDate, MarchingOnRoad, MarchingStatus, MarchingToLand,
};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;

use super::super::TITLE;

pub(super) fn render_armies_spans(world: &mut World, kingdom_e: Entity) -> Vec<(String, Color)> {
    let armies: Vec<Entity> = world
        .get::<KingdomHasArmies>(kingdom_e)
        .map(|k| k.iter().collect())
        .unwrap_or_default();
    if armies.is_empty() {
        return Vec::new();
    }
    let mut armies_q = world.query::<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        &ArmyMaxLevy,
        Option<&ArmyMarching>,
    )>();
    let mut army_queues_q = world.query::<&ArmyHasMarching>();
    let mut army_sieges_q = world.query::<&ArmyHasSiege>();
    let mut army_marching_q = world.query::<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>();
    let mut siege_q = world.query::<&SiegeProgress>();
    let mut roads_q = world.query::<&RoadDistanceDays>();
    let mut lands_q = world.query::<&LandName>();
    let calendar = world.resource::<Calendar>().clone();
    let date = world.resource::<Date>().clone();
    let mut lines: Vec<String> = Vec::new();
    for army_e in armies {
        if let Some(line) = army_line(
            army_e,
            &mut armies_q,
            &mut army_queues_q,
            &mut army_sieges_q,
            &mut army_marching_q,
            &mut siege_q,
            &mut roads_q,
            &mut lands_q,
            world,
            &calendar,
            &date,
        ) {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("armies:\n".to_string(), TITLE)];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((line.clone(), Color::WHITE));
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

// ponytail: duplicated from ui/army.rs — the format is one small match arm,
// and the two call sites want exactly the same string today. Pull a shared
// helper into ui/army.rs the moment a third caller appears or the format
// starts branching between the panels.
#[allow(clippy::too_many_arguments)]
fn army_line(
    army_e: Entity,
    armies: &mut bevy::ecs::query::QueryState<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        &ArmyMaxLevy,
        Option<&ArmyMarching>,
    )>,
    army_queues: &mut bevy::ecs::query::QueryState<&ArmyHasMarching>,
    army_sieges: &mut bevy::ecs::query::QueryState<&ArmyHasSiege>,
    army_marching: &mut bevy::ecs::query::QueryState<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    siege_progress: &mut bevy::ecs::query::QueryState<&SiegeProgress>,
    roads: &mut bevy::ecs::query::QueryState<&RoadDistanceDays>,
    lands: &mut bevy::ecs::query::QueryState<&LandName>,
    world: &mut World,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let (name, levy, on_land, status, max_levy, current_marching) =
        armies.get(world, army_e).ok()?;
    let current_land = lands
        .get(world, on_land.0)
        .ok()
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let base = format!("{} ({}) at {}", name.0, levy.0, current_land);
    match status {
        ArmyStatus::Idle => Some(base),
        ArmyStatus::Raising => Some(format!("{base} raising {}/{}", levy.0, max_levy.0)),
        ArmyStatus::Marching => {
            let queue = army_queues.get(world, army_e).ok()?;
            let hops: Vec<_> = queue.iter().collect();
            let (final_dest, total_days) = route_summary(
                &hops,
                current_marching.copied().map(|m| m.0),
                army_marching,
                roads,
                lands,
                world,
                calendar,
                date,
            );
            Some(format!("{base} marching to {final_dest} at {total_days} days"))
        }
        ArmyStatus::Sieging => {
            let progress = army_sieges
                .get(world, army_e)
                .ok()
                .and_then(|ahs| siege_progress.get(world, ahs.siege()).ok())
                .map(|sp| sp.0)
                .unwrap_or(0);
            Some(format!("{base} sieging at {progress}%"))
        }
    }
}

fn route_summary(
    hops: &[Entity],
    current_marching: Option<Entity>,
    army_marching: &mut bevy::ecs::query::QueryState<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    roads: &mut bevy::ecs::query::QueryState<&RoadDistanceDays>,
    lands: &mut bevy::ecs::query::QueryState<&LandName>,
    world: &World,
    calendar: &Calendar,
    date: &Date,
) -> (String, i64) {
    let today_ord = date.ordinal(calendar);
    let on_route_days: i64 = current_marching
        .and_then(|cur| army_marching.get(world, cur).ok())
        .and_then(|(_, _, _, arrived_opt)| arrived_opt.and_then(|d| d.0))
        .map(|arrived| (arrived.ordinal(calendar) - today_ord).max(0))
        .unwrap_or(0);
    let mut total_days: i64 = 0;
    for &hop in hops {
        if let Ok((_, _, on_road, _)) = army_marching.get(world, hop)
            && let Some(road_distance_days) = roads.get(world, on_road.0).ok()
        {
            total_days += road_distance_days.0 as i64;
        }
    }
    if current_marching.is_some()
        && let Some(cur) = current_marching
        && let Ok((_, _, on_road, _)) = army_marching.get(world, cur)
        && let Some(road_distance_days) = roads.get(world, on_road.0).ok()
    {
        total_days -= road_distance_days.0 as i64;
    }
    total_days += on_route_days;
    let final_dest = hops
        .last()
        .and_then(|&h| army_marching.get(world, h).ok())
        .and_then(|(_, to, _, _)| lands.get(world, to.0).ok())
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "?".into());
    (final_dest, total_days)
}

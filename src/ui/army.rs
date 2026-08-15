//! The ARMIES panel: every army the player's kingdom has raised. Hidden when
//! the player rules no kingdom or the kingdom has no armies.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::army::{ArmyHasMarching, ArmyHasSiege, ArmyMarching, ArmyStatus};
use crate::ecs::marching::{MarchingArrivedDate, MarchingOnRoad, MarchingStatus, MarchingToLand};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::ecs::{
    ArmyLevy, ArmyMaxLevy, ArmyName, ArmyOnLand, CharacterLeads, KingdomHasArmies, LandName,
    Registry,
};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Marker on the ARMIES panel's body text.
#[derive(Component)]
pub struct UIWithArmies;

pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(panel),
    ))
    .with_children(|p| {
        p.spawn((Text::new("ARMIES"), TextFont::from_font_size(FONT), TextColor(TITLE)));
        p.spawn((
            Text::new(""),
            TextFont::from_font_size(FONT),
            TextColor(Color::WHITE),
            UIWithArmies,
        ));
    });
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    mut bodies: Query<(Entity, &mut Text), With<UIWithArmies>>,
    parents: Query<&ChildOf>,
    mut nodes: Query<&mut Node>,
    armies: Query<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        &ArmyMaxLevy,
        Option<&ArmyMarching>,
    )>,
    army_queues: Query<&ArmyHasMarching>,
    army_sieges: Query<&ArmyHasSiege>,
    marchings: Query<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    siege_progress: Query<&SiegeProgress>,
    roads: Query<&RoadDistanceDays>,
    lands: Query<&LandName>,
    player_chars: Query<&CharacterLeads>,
    kingdom_armies: Query<&KingdomHasArmies>,
    calendar: Res<Calendar>,
    date: Res<Date>,
) {
    let army_lines: Vec<String> = game
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|player_e| player_chars.get(player_e).ok())
        .map(|character_leads| {
            let mut out = Vec::new();
            for kingdom_e in character_leads.kingdoms() {
                let Ok(kingdom_has_armies) = kingdom_armies.get(*kingdom_e) else { continue };
                for army_e in kingdom_has_armies.iter() {
                    if let Some(line) = format_army_line(
                        army_e, &armies, &army_queues, &army_sieges, &marchings,
                        &siege_progress, &roads, &lands, &calendar, &date,
                    ) {
                        out.push(line);
                    }
                }
            }
            out
        })
        .unwrap_or_default();

    let visible = !army_lines.is_empty();
    let display = if visible { Display::Flex } else { Display::None };
    for (body_e, mut body) in &mut bodies {
        if let Ok(child_of) = parents.get(body_e)
            && let Ok(mut node) = nodes.get_mut(child_of.parent())
        {
            node.display = display;
        }
        body.0 = army_lines.join("\n");
    }
}

/// Build one army's panel line from its live entity data. `None` when required components are missing.
#[allow(clippy::too_many_arguments)]
fn format_army_line(
    army_e: bevy::ecs::entity::Entity,
    armies: &Query<(
        &ArmyName, &ArmyLevy, &ArmyOnLand, &ArmyStatus, &ArmyMaxLevy, Option<&ArmyMarching>,
    )>,
    army_queues: &Query<&ArmyHasMarching>,
    army_sieges: &Query<&ArmyHasSiege>,
    marchings: &Query<(
        &MarchingStatus, &MarchingToLand, &MarchingOnRoad, Option<&MarchingArrivedDate>,
    )>,
    siege_progress: &Query<&SiegeProgress>,
    roads: &Query<&RoadDistanceDays>,
    lands: &Query<&LandName>,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let (name, levy, on_land, status, max_levy, current_marching) = armies.get(army_e).ok()?;
    let current_land = lands.get(on_land.0).ok().map(|land_name| land_name.0.clone()).unwrap_or_else(|| "?".into());
    let base = format!("{} ({}) at {}", name.0, levy.0, current_land);

    match status {
        ArmyStatus::Idle => Some(base),
        ArmyStatus::Raising => Some(format!("{base} raising {}/{}", levy.0, max_levy.0)),
        ArmyStatus::Marching => {
            let queue = army_queues.get(army_e).ok()?;
            let hops: Vec<_> = queue.iter().collect();
            let (final_dest, total_days) = route_summary(
                &hops, current_marching.copied().map(|m| m.0),
                marchings, roads, lands, calendar, date,
            );
            Some(format!("{base} marching to {final_dest} at {total_days} days"))
        }
        ArmyStatus::Sieging => {
            let progress = army_sieges
                .get(army_e)
                .ok()
                .and_then(|army_has_siege| siege_progress.get(army_has_siege.siege()).ok())
                .map(|siege_progress| siege_progress.0)
                .unwrap_or(0);
            Some(format!("{base} sieging at {progress}%"))
        }
    }
}

/// Reduce the army's marching queue to `(final_destination, total_days)`.
#[allow(clippy::too_many_arguments)]
fn route_summary(
    hops: &[bevy::ecs::entity::Entity],
    current_marching: Option<bevy::ecs::entity::Entity>,
    marchings: &Query<(
        &MarchingStatus, &MarchingToLand, &MarchingOnRoad, Option<&MarchingArrivedDate>,
    )>,
    roads: &Query<&RoadDistanceDays>,
    lands: &Query<&LandName>,
    calendar: &Calendar,
    date: &Date,
) -> (String, i64) {
    let today_ord = date.ordinal(calendar);

    let on_route_days: i64 = current_marching
        .and_then(|cur| marchings.get(cur).ok())
        .and_then(|(_, _, _, arrived_opt)| arrived_opt.and_then(|d| d.0))
        .map(|arrived| (arrived.ordinal(calendar) - today_ord).max(0))
        .unwrap_or(0);

    let mut total_days: i64 = 0;
    for &hop in hops {
        if let Ok((_, _, on_road, _)) = marchings.get(hop)
            && let Some(road_distance_days) = roads.get(on_road.0).ok()
        {
            total_days += road_distance_days.0 as i64;
        }
    }
    // Net out the OnRoute hop's full road cost — it was counted once in the loop AND once as live `on_route_days`.
    if current_marching.is_some()
        && let Some(cur) = current_marching
        && let Ok((_, _, on_road, _)) = marchings.get(cur)
        && let Some(road_distance_days) = roads.get(on_road.0).ok()
    {
        total_days -= road_distance_days.0 as i64;
    }
    total_days += on_route_days;

    let final_dest = hops
        .last()
        .and_then(|&h| marchings.get(h).ok())
        .and_then(|(_, to, _, _)| lands.get(to.0).ok())
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "?".into());

    (final_dest, total_days)
}

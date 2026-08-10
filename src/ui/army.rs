//! The ARMIES panel at the top of the right-hand column (just below the
//! WARS panel): every army the player's kingdom has raised. Hidden when
//! the player rules no kingdom, or the kingdom has no armies (via
//! `Display::None` on the outer node so it leaves no gap in the column
//! layout).
//!
//! The list walks `actor → CharacterLeads → kingdom → KingdomHasArmies` —
//! the auto-maintained reverse of `ArmyBelongsToKingdom`. Each line reads
//! the army's live entity data:
//!
//! - **Idle** — `"<name> (<levy>) at <land>"`.
//! - **Marching** — `"<name> (<levy>) at <land> marching to <final_dest> at <days> days"`,
//!   where `<final_dest>` is the LAST marching in the army's
//!   `ArmyHasMarching` queue (the player's queued destination, not the
//!   next hop), and `<days>` is the total remaining march time: days left
//!   on the currently `OnRoute` hop plus each subsequent `Scheduled`
//!   hop's `RoadDistanceDays`.
//! - **Sieging** — `"<name> (<levy>) at <land> sieging at <progress>%"`,
//!   where `<progress>` is the siege's `SiegeProgress` (0–100; resolves
//!   the land to the attacker when it hits 100).

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::army::{ArmyHasMarching, ArmyHasSiege, ArmyMarching, ArmyStatus};
use crate::ecs::marching::{MarchingArrivedDate, MarchingOnRoad, MarchingStatus, MarchingToLand};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::ecs::{
    ArmyLevy, ArmyName, ArmyOnLand, CharacterLeads, KingdomHasArmies, LandName, Registry,
};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Marker on the ARMIES panel's body text. Same shape as
/// [`super::wars::UIWithWars`]: the container's visibility is toggled by
/// walking the body's [`ChildOf`].
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
        p.spawn((
            Text::new("ARMIES"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
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
    // Query<(Entity, &mut Text)> so we get the body's entity alongside the
    // mutable text — the parent walk needs the body entity to find the
    // container. Iterate, don't `single_mut`: the body should always be
    // there (spawned at startup), but a `for` loop degrades gracefully if
    // it isn't, whereas `single_mut` would panic.
    mut bodies: Query<(Entity, &mut Text), With<UIWithArmies>>,
    // `ChildOf` is Bevy 0.19's renamed `Parent` component (this is what
    // every UI child gets auto-spawned with). The body text's parent is
    // the panel's container node — toggling its `Display` hides the whole
    // panel.
    parents: Query<&ChildOf>,
    mut nodes: Query<&mut Node>,
    armies: Query<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
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
    // Player → kingdom → armies. Same shape as the WARS walk.
    let army_lines: Vec<String> = registry
        .get(&game.ctx.player_character_id)
        .and_then(|player_e| player_chars.get(player_e).ok())
        .map(|character_leads| character_leads.kingdom())
        .and_then(|kingdom_e| kingdom_armies.get(kingdom_e).ok())
        .map(|kingdom_has_armies| {
            kingdom_has_armies
                .iter()
                .filter_map(|army_e| {
                    format_army_line(
                        army_e,
                        &armies,
                        &army_queues,
                        &army_sieges,
                        &marchings,
                        &siege_progress,
                        &roads,
                        &lands,
                        &calendar,
                        &date,
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let visible = !army_lines.is_empty();
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    for (body_e, mut body) in &mut bodies {
        if let Ok(child_of) = parents.get(body_e)
            && let Ok(mut node) = nodes.get_mut(child_of.parent())
        {
            node.display = display;
        }
        body.0 = army_lines.join("\n");
    }
}

/// Build one army's panel line from its live entity data. Three shapes:
///
/// - **Idle** — `"<name> (<levy>) at <land>"`.
/// - **Marching** — `"<name> (<levy>) at <land> marching to <final_dest> at <days> days"`.
/// - **Sieging** — `"<name> (<levy>) at <land> sieging at <progress>%"`,
///   reading the siege's [`SiegeProgress`] through the army's `ArmyHasSiege`
///   reverse target.
///
/// `None` when the army's required components are missing — the caller
/// filters those out so a torn-world army doesn't crash the panel.
#[allow(clippy::too_many_arguments)]
fn format_army_line(
    army_e: bevy::ecs::entity::Entity,
    armies: &Query<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        Option<&ArmyMarching>,
    )>,
    army_queues: &Query<&ArmyHasMarching>,
    army_sieges: &Query<&ArmyHasSiege>,
    marchings: &Query<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    siege_progress: &Query<&SiegeProgress>,
    roads: &Query<&RoadDistanceDays>,
    lands: &Query<&LandName>,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let (name, levy, on_land, status, current_marching) = armies.get(army_e).ok()?;
    let current_land = lands
        .get(on_land.0)
        .ok()
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let base = format!("{} ({}) at {}", name.0, levy.0, current_land);

    match status {
        ArmyStatus::Idle => Some(base),
        ArmyStatus::Marching => {
            let queue = army_queues.get(army_e).ok()?;
            let hops: Vec<_> = queue.iter().collect();
            let (final_dest, total_days) = route_summary(
                &hops,
                current_marching.copied().map(|m| m.0),
                marchings,
                roads,
                lands,
                calendar,
                date,
            );
            Some(format!("{base} marching to {final_dest} at {total_days} days"))
        }
        ArmyStatus::Sieging => {
            // The siege progress sits on the siege entity, reachable
            // through `ArmyHasSiege` (the auto-maintained reverse of
            // `SiegeAttackerArmy`). If the link is missing — the siege
            // was just despawned by the tick, or a torn world — fall
            // back to "sieging" with no progress so the line still
            // shows the army is busy.
            let progress = army_sieges
                .get(army_e)
                .ok()
                .and_then(|army_has_siege| {
                    siege_progress.get(army_has_siege.siege()).ok()
                })
                .map(|siege_progress| siege_progress.0)
                .unwrap_or(0);
            Some(format!("{base} sieging at {progress}%"))
        }
    }
}

/// Reduce the army's marching queue to `(final_destination, total_days)`.
/// `current_marching` is the `ArmyMarching` pointer (the `OnRoute` hop);
/// `None` is a degraded state (Marching army with no current marching)
/// that the helper handles by summing every hop's road cost.
#[allow(clippy::too_many_arguments)]
fn route_summary(
    hops: &[bevy::ecs::entity::Entity],
    current_marching: Option<bevy::ecs::entity::Entity>,
    marchings: &Query<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    roads: &Query<&RoadDistanceDays>,
    lands: &Query<&LandName>,
    calendar: &Calendar,
    date: &Date,
) -> (String, i64) {
    let today_ord = date.ordinal(calendar);

    // Days left on the currently `OnRoute` hop: its `MarchingArrivedDate`
    // minus today, clamped to ≥ 0. Missing arrived date (e.g. mid-route
    // glitch) → 0.
    let on_route_days: i64 = current_marching
        .and_then(|cur| marchings.get(cur).ok())
        .and_then(|(_, _, _, arrived_opt)| arrived_opt.and_then(|d| d.0))
        .map(|arrived| (arrived.ordinal(calendar) - today_ord).max(0))
        .unwrap_or(0);

    // Sum each hop's `RoadDistanceDays`. Includes the OnRoute hop too —
    // subtracting `on_route_days`'s share via the post-loop adjustment
    // below avoids double-counting the live hop when `current_marching`
    // is present. With no `current_marching` (degraded) the full sum is
    // the route's total road cost, which is the sensible read.
    let mut total_days: i64 = 0;
    for &hop in hops {
        if let Ok((_, _, on_road, _)) = marchings.get(hop)
            && let Some(road_distance_days) = roads.get(on_road.0).ok()
        {
            total_days += road_distance_days.0 as i64;
        }
    }
    // Net out the OnRoute hop's full road cost — it was counted once in
    // the loop AND once as the live `on_route_days` partial. Subtract the
    // full `RoadDistanceDays` so the live `on_route_days` stands alone.
    if current_marching.is_some()
        && let Some(cur) = current_marching
        && let Ok((_, _, on_road, _)) = marchings.get(cur)
        && let Some(road_distance_days) = roads.get(on_road.0).ok()
    {
        total_days -= road_distance_days.0 as i64;
    }
    total_days += on_route_days;

    // Final destination = last hop's `MarchingToLand` land name. Empty
    // queue falls back to "?" — shouldn't happen for a Marching army but
    // the panel must not panic.
    let final_dest = hops
        .last()
        .and_then(|&h| marchings.get(h).ok())
        .and_then(|(_, to, _, _)| lands.get(to.0).ok())
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "?".into());

    (final_dest, total_days)
}

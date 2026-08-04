//! The legend panel above the chronicle: what the map selection is.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingsOn, CharacterAge, CharacterName, Holds, HouseName, HouseOf, LandName,
    LedBy, Registry, Seat, StringId,
};
use crate::resources::buildings::BuildingDefs;
use bevy::prelude::*;

/// id / land / kingdom detail block.
#[derive(Component)]
pub struct LegendInfo;
/// Buildings list and total yield block.
#[derive(Component)]
pub struct LegendBuildings;

/// Faint rule between the two sections.
const DIVIDER: Color = Color::srgba(0.5, 0.5, 0.5, 0.6);

/// Fills the space the chronicle leaves in the right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("LEGEND"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((LegendInfo, Text::new(""), TextFont::from_font_size(FONT)));
        // Section divider: a thin rule with vertical margin.
        p.spawn((
            Node {
                width: percent(100),
                height: px(1),
                margin: UiRect::vertical(px(6)),
                ..default()
            },
            BackgroundColor(DIVIDER),
        ));
        p.spawn((
            LegendBuildings,
            Text::new(""),
            TextFont::from_font_size(FONT),
        ));
    });
}

#[allow(clippy::type_complexity)]
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    defs: Res<BuildingDefs>,
    mut info: Single<&mut Text, (With<LegendInfo>, Without<LegendBuildings>)>,
    mut bld: Single<&mut Text, (With<LegendBuildings>, Without<LegendInfo>)>,
    lands: Query<(&LandName, &BuildingsOn)>,
    buildings: Query<&BuildingOf>,
    kingdoms: Query<(&StringId, &Holds, Option<&Seat>, Option<&LedBy>)>,
    chars: Query<(&CharacterName, &CharacterAge)>,
    house_of: Query<&HouseOf>,
    houses: Query<&HouseName>,
) {
    // Nothing selected, or a selected id the world doesn't have: blank both.
    let Some(id) = game.ctx.selected_land_id.clone() else {
        info.0.clear();
        bld.0.clear();
        return;
    };
    let Some(land_e) = registry.get(&id) else {
        info.0.clear();
        bld.0.clear();
        return;
    };
    let Ok((land, on)) = lands.get(land_e) else {
        info.0.clear();
        bld.0.clear();
        return;
    };

    // Section 1: id, land, kingdom detail.
    let mut inf = format!("id:{id}\nname:{}", land.0);
    if let Some((k_sid, _holds, seat, leader)) =
        kingdoms.iter().find(|(_, h, _, _)| h.iter().any(|e| e == land_e))
    {
        inf.push_str(&format!("\nkingdom:{}", k_sid.0));
        if seat.is_some_and(|s| s.0 == land_e) {
            inf.push_str(" (seat)");
        }
        if let Some(leader) = leader
            && let Ok((ch, cs)) = chars.get(leader.0)
        {
            let house = house_of
                .get(leader.0)
                .ok()
                .and_then(|ho| houses.get(ho.0).ok())
                .map(|h| h.0.clone())
                .unwrap_or_default();
            inf.push_str(&format!("\nruler:{} of {} ({})", ch.0, house, cs.0));
        }
    }
    info.0 = inf;

    // Section 2: per-building yield and total — walk the land's building
    // instances through to their definitions for the stats.
    let (mut gold, mut levy) = (0i64, 0u64);
    let mut out = String::new();
    for b_e in on.iter() {
        let Some(d) = buildings.get(b_e).ok().and_then(|of| defs.get(&of.0)) else {
            continue;
        };
        gold += d.gold_profit as i64 - d.gold_upkeep as i64;
        levy += d.levy as u64;
        // ponytail: only the non-zero numbers, so a line reads
        // "market square +10g" not "+10g -0g 0 levy".
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("- {}", d.name));
        if d.gold_profit > 0 {
            out.push_str(&format!(" +{}g", d.gold_profit));
        }
        if d.gold_upkeep > 0 {
            out.push_str(&format!(" -{}g", d.gold_upkeep));
        }
        if d.levy > 0 {
            out.push_str(&format!(" {} levy", d.levy));
        }
    }
    if !out.is_empty() {
        out.push('\n');
        out.push_str(&format!("total: {gold:+}g {levy} levy"));
    }
    bld.0 = out;
}

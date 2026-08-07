//! The BUILDINGS panel in the right-hand column: the selected land's
//! buildings as a 3-column table (name / gold / levy) with a totals row.
//!
//! Name colour reflects each building's pool state: red while its levy is
//! raised into an army (`BuildingIsRaised`), yellow while the pool is
//! partially drained (`BuildingLevy < def.levy`, displayed as
//! `Name (current/max)`), and white when the pool is full. Non-ACTIVE
//! buildings stay greyed-out as before.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus,
    LandHasBuildings, Registry,
};
use crate::resources::buildings::BuildingDefs;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Buildings list and total yield block. A column container; its child rows
/// are rebuilt by [`update`] only when the selection or building roster changes.
#[derive(Component)]
pub struct LegendBuildings;

/// Faint rule above the totals row.
const DIVIDER: Color = Color::srgba(0.5, 0.5, 0.5, 0.6);
/// Fixed widths so the gold and levy columns line up across rows.
const GOLD_W: f32 = 48.0;
const LEVY_W: f32 = 40.0;
/// Greyed-out colour for non-ACTIVE rows (under construction / inactive).
const BUILDING_GREY: Color = Color::srgba(0.55, 0.55, 0.55, 1.0);
/// Name colour for ACTIVE buildings whose levy is currently in an army.
const RAISED_RED: Color = Color::Srgba(css::RED);
/// Name colour for ACTIVE buildings whose pool is partially drained but
/// not currently raised (mid-replenishment, or post-dismiss before a full
/// month has rolled over).
const PARTIAL_YELLOW: Color = Color::Srgba(css::YELLOW);

/// The BUILDINGS panel: title + column container of building rows. Spawned
/// as a sibling panel below `information` in the right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            // Grows to fill whatever `information` leaves in the column
            // (buildings sits above `actions` + `chronicle`, which are pinned).
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("BUILDINGS"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((
            LegendBuildings,
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(1),
                ..default()
            },
        ));
    });
}

/// One table row: name left-aligned filling the space, gold and levy
/// right-aligned in fixed-width cells so the columns line up across rows.
/// `name_color` tints the name cell; `value_color` tints gold + levy.
/// Non-ACTIVE rows pass the same grey for both; ACTIVE rows pass white
/// for the cells and either white / yellow / red for the name.
fn row(
    p: &mut ChildSpawnerCommands,
    name: &str,
    gold: &str,
    levy: &str,
    name_color: Color,
    value_color: Color,
) {
    p.spawn(Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        ..default()
    })
    .with_children(|r| {
        r.spawn((
            Text::new(name.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(name_color),
            Node {
                flex_grow: 1.0,
                ..default()
            },
        ));
        r.spawn((
            Text::new(gold.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(value_color),
            TextLayout::justify(Justify::Right),
            Node {
                width: px(GOLD_W),
                ..default()
            },
        ));
        r.spawn((
            Text::new(levy.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(value_color),
            TextLayout::justify(Justify::Right),
            Node {
                width: px(LEVY_W),
                ..default()
            },
        ));
    });
}

/// Rebuild the table only when its key changes, so the rows aren't respawned
/// every frame. A `None` key (no selection, or no buildings) clears it.
fn rebuild(
    commands: &mut Commands,
    container: Entity,
    key: &mut Option<String>,
    cur: Option<String>,
    rows: &[(String, String, String, Color, Color)],
    total: Option<(String, String)>,
) {
    if *key == cur {
        return;
    }
    let has_rows = cur.is_some();
    *key = cur;
    commands.entity(container).despawn_children();
    if !has_rows {
        return;
    }
    commands.entity(container).with_children(|p| {
        for (name, gold, levy, name_color, value_color) in rows {
            row(p, name, gold, levy, *name_color, *value_color);
        }
        // A faint rule, then the totals in the same 3-column layout.
        p.spawn((
            Node {
                width: percent(100),
                height: px(1),
                margin: UiRect::vertical(px(3)),
                ..default()
            },
            BackgroundColor(DIVIDER),
        ));
        if let Some((gold, levy)) = total {
            row(p, "total", &gold, &levy, Color::WHITE, Color::WHITE);
        }
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    defs: Res<BuildingDefs>,
    container: Single<Entity, With<LegendBuildings>>,
    // ponytail: cache key in a Local so identical selections don't respawn the
    // table every frame; it flips only on a new land or a changed building set.
    mut key: Local<Option<String>>,
    mut commands: Commands,
    lands: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    building_finish: Query<&BuildingConstructionDate>,
    building_levy: Query<&BuildingLevy>,
    building_is_raised: Query<&BuildingIsRaised>,
) {
    // Nothing selected, or a selected id the world doesn't resolve to a land:
    // blank the buildings table. The information panel clears itself
    // independently.
    let Some((id, land_has_buildings)) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id).map(|e| (id.clone(), e)))
        .and_then(|(id, e)| lands.get(e).ok().map(|l| (id, l)))
    else {
        rebuild(&mut commands, *container, &mut key, None, &[], None);
        return;
    };

    // Per-building yield and total — walk the land's building instances
    // through to their definitions for the stats. Buildings still under
    // construction (`status == BUILDING`) are listed in grey with
    // `Name (YYYY.MM.DD)`; `INACTIVE` buildings are listed with no yield
    // info. Only `ACTIVE` buildings contribute to the totals; for those,
    // the name colour reflects the pool state (`BuildingIsRaised` →
    // red; partial `BuildingLevy` → yellow with `Name (current/max)`;
    // full pool → white).
    let (mut gold, mut levy) = (0i64, 0u64);
    let mut rows: Vec<(String, String, String, Color, Color)> = Vec::new();
    // The key tracks selection + the building roster on it (def ids + per-
    // building status + finish date + pool state); it changes when the
    // land, the building set, or any visible pool state does.
    let mut sig = String::new();
    for b_e in land_has_buildings.iter() {
        let Ok(building_of) = building_of.get(b_e) else {
            continue;
        };
        let Some(d) = defs.get(&building_of.0) else {
            continue;
        };
        let status = building_status
            .get(b_e)
            .copied()
            .unwrap_or(BuildingStatus::Active);
        let finish = building_finish.get(b_e).ok().map(|f| f.0);
        let is_raised = building_is_raised
            .get(b_e)
            .copied()
            .unwrap_or(BuildingIsRaised(false))
            .0;
        let current_levy = building_levy
            .get(b_e)
            .copied()
            .unwrap_or(BuildingLevy(0))
            .0;
        let max_levy = d.levy;
        let active = status == BuildingStatus::Active;
        if !sig.is_empty() {
            sig.push(',');
        }
        sig.push_str(&format!(
            "{}:{:?}:{}:{}:{}",
            building_of.0,
            status,
            finish
                .map(|building_construction_date| building_construction_date.to_string())
                .unwrap_or_default(),
            is_raised,
            current_levy,
        ));

        // Cells. ACTIVE buildings show their def's gold/levy values; non-
        // ACTIVE rows leave the cells empty. Cells stay `value_color` (white
        // for ACTIVE, grey for non-ACTIVE) regardless of the name's state —
        // the cell represents "what this building contributes to the
        // realm's standing pool" (a fixed per-def number), which is
        // independent of whether the levy is currently raised into an army.
        let (g_cell, l_cell);
        if active {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
            // One gold field is ever set (profit xor upkeep), so the net is
            // whichever sign is present; omit zeroes to keep a cell clean.
            g_cell = if d.gold_profit > 0 {
                format!("+{}g", d.gold_profit)
            } else if d.gold_upkeep > 0 {
                format!("-{}g", d.gold_upkeep)
            } else {
                String::new()
            };
            l_cell = if d.levy > 0 {
                d.levy.to_string()
            } else {
                String::new()
            };
        } else {
            g_cell = String::new();
            l_cell = String::new();
        }

        // Name + colour. Priority order: raised > partial > full > non-
        // ACTIVE. Raised is a strict subset of "pool is drained", but we
        // show red (no format) over yellow (`Name (current/max)`) because
        // the flag communicates a different fact: "this levy is currently
        // in the field" vs "the pool is mid-replenish".
        let (name_color, value_color, display_name) = match status {
            BuildingStatus::Building => {
                let name = format!(
                    "{} ({})",
                    d.name,
                    finish
                        .map(|building_construction_date| building_construction_date.to_string())
                        .unwrap_or_else(|| "?".into())
                );
                (BUILDING_GREY, BUILDING_GREY, name)
            }
            BuildingStatus::Inactive => (BUILDING_GREY, BUILDING_GREY, d.name.clone()),
            BuildingStatus::Active => {
                if is_raised {
                    (RAISED_RED, Color::WHITE, d.name.clone())
                } else if current_levy < max_levy {
                    (
                        PARTIAL_YELLOW,
                    Color::WHITE,
                    format!("{} ({}/{})", d.name, current_levy, max_levy),
                    )
                } else {
                    (Color::WHITE, Color::WHITE, d.name.clone())
                }
            }
        };
        rows.push((display_name, g_cell, l_cell, name_color, value_color));
    }

    let (cur_key, total) = if rows.is_empty() {
        (None, None)
    } else {
        (
            Some(format!("{id}|{sig}")),
            Some((format!("{gold:+}g"), levy.to_string())),
        )
    };
    rebuild(&mut commands, *container, &mut key, cur_key, &rows, total);
}
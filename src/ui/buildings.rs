//! The BUILDINGS panel: the selected land's buildings as a 3-column table
//! (name / gold / levy) with a totals row. Name colour reflects pool state:
//! red when raised, yellow when partial, white when full; non-ACTIVE rows grey.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus,
    LandHasBuildings, Registry,
};
use crate::resources::buildings::BuildingDefs;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Buildings list and total yield block. A column container; its child rows
/// are rebuilt by `update` only when the selection or building roster changes.
#[derive(Component)]
pub struct LegendBuildings;

const DIVIDER: Color = Color::srgba(0.5, 0.5, 0.5, 0.6);
const GOLD_W: f32 = 48.0;
const LEVY_W: f32 = 40.0;
const BUILDING_GREY: Color = Color::srgba(0.55, 0.55, 0.55, 1.0);
const RAISED_RED: Color = Color::Srgba(css::RED);
const PARTIAL_YELLOW: Color = Color::Srgba(css::YELLOW);

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
        p.spawn((Text::new("BUILDINGS"), TextFont::from_font_size(FONT), TextColor(TITLE)));
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
            Node { flex_grow: 1.0, ..default() },
        ));
        r.spawn((
            Text::new(gold.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(value_color),
            TextLayout::justify(Justify::Right),
            Node { width: px(GOLD_W), ..default() },
        ));
        r.spawn((
            Text::new(levy.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(value_color),
            TextLayout::justify(Justify::Right),
            Node { width: px(LEVY_W), ..default() },
        ));
    });
}

/// Rebuild the table only when its key changes.
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
    calendar: Res<Calendar>,
    date: Res<Date>,
    container: Single<Entity, With<LegendBuildings>>,
    mut key: Local<Option<String>>,
    mut commands: Commands,
    lands: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    building_finish: Query<&BuildingConstructionDate>,
    building_levy: Query<&BuildingLevy>,
    building_is_raised: Query<&BuildingIsRaised>,
) {
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

    let (mut gold, mut levy) = (0i64, 0u64);
    let mut rows: Vec<(String, String, String, Color, Color)> = Vec::new();
    let mut sig = String::new();
    for b_e in land_has_buildings.iter() {
        let Ok(building_of) = building_of.get(b_e) else { continue };
        let Some(d) = defs.get(&building_of.0) else { continue };
        let status = building_status.get(b_e).copied().unwrap_or(BuildingStatus::Active);
        let finish = building_finish.get(b_e).ok().map(|f| f.0);
        let is_raised = building_is_raised.get(b_e).copied().unwrap_or(BuildingIsRaised(false)).0;
        let current_levy = building_levy.get(b_e).copied().unwrap_or(BuildingLevy(0)).0;
        let max_levy = d.levy;
        let active = status == BuildingStatus::Active;
        if !sig.is_empty() {
            sig.push(',');
        }
        sig.push_str(&format!(
            "{}:{:?}:{}:{}:{}:{}",
            building_of.0,
            status,
            finish.map(|d| d.to_string()).unwrap_or_default(),
            is_raised,
            current_levy,
            date.tick_count,
        ));

        let (g_cell, l_cell);
        if active {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
            g_cell = if d.gold_profit > 0 {
                format!("+{}g", d.gold_profit)
            } else if d.gold_upkeep > 0 {
                format!("-{}g", d.gold_upkeep)
            } else {
                String::new()
            };
            l_cell = if d.levy > 0 { d.levy.to_string() } else { String::new() };
        } else {
            g_cell = String::new();
            l_cell = String::new();
        }

        let (name_color, value_color, display_name) = match status {
            BuildingStatus::Building => {
                let name = format!(
                    "{} ({})",
                    d.name,
                    finish
                        .map(|f| {
                            let remaining = (f.ordinal(&calendar) - date.ordinal(&calendar)).max(0) as u32;
                            calendar.format_duration(remaining)
                        })
                        .unwrap_or_else(|| "?".into())
                );
                (BUILDING_GREY, BUILDING_GREY, name)
            }
            BuildingStatus::Inactive => (BUILDING_GREY, BUILDING_GREY, d.name.clone()),
            BuildingStatus::Active => {
                if is_raised {
                    (RAISED_RED, Color::WHITE, d.name.clone())
                } else if current_levy < max_levy {
                    (PARTIAL_YELLOW, Color::WHITE, format!("{} ({}/{})", d.name, current_levy, max_levy))
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

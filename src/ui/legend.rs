//! The legend panel above the chronicle: information about the selected
//! land and its buildings.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    BuildingOf, CharacterAge, CharacterName, CharacterOfHouse, HouseName, KingdomHolds,
    KingdomLedBy, KingdomSeat, LandHasBuildings, LandName, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use bevy::prelude::*;

/// id / land / kingdom detail block.
#[derive(Component)]
pub struct LegendInfo;
/// Buildings list and total yield block. A column container; its child rows
/// are rebuilt by [`update`] only when the selection or building roster changes.
#[derive(Component)]
pub struct LegendBuildings;

/// Faint rule between the two sections.
const DIVIDER: Color = Color::srgba(0.5, 0.5, 0.5, 0.6);
/// Fixed widths so the gold and levy columns line up across rows.
const GOLD_W: f32 = 48.0;
const LEVY_W: f32 = 40.0;

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
            Text::new("INFORMATION"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((
            Node {
                width: percent(100),
                height: px(1),
                margin: UiRect::vertical(px(6)),
                ..default()
            },
            BackgroundColor(DIVIDER),
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
            Text::new("BUILDINGS"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((
            Node {
                width: percent(100),
                height: px(1),
                margin: UiRect::vertical(px(6)),
                ..default()
            },
            BackgroundColor(DIVIDER),
        ));
        // The per-building table: a column of rows, each split into
        // name (left, fills) / gold (right) / levy (right).
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
fn row(p: &mut ChildSpawnerCommands, name: &str, gold: &str, levy: &str) {
    p.spawn(Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        ..default()
    })
    .with_children(|r| {
        r.spawn((
            Text::new(name.to_string()),
            TextFont::from_font_size(FONT),
            Node {
                flex_grow: 1.0,
                ..default()
            },
        ));
        r.spawn((
            Text::new(gold.to_string()),
            TextFont::from_font_size(FONT),
            TextLayout::justify(Justify::Right),
            Node {
                width: px(GOLD_W),
                ..default()
            },
        ));
        r.spawn((
            Text::new(levy.to_string()),
            TextFont::from_font_size(FONT),
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
    rows: &[(String, String, String)],
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
        for (name, gold, levy) in rows {
            row(p, name, gold, levy);
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
            row(p, "total", &gold, &levy);
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    defs: Res<BuildingDefs>,
    mut info: Single<&mut Text, (With<LegendInfo>, Without<LegendBuildings>)>,
    container: Single<Entity, With<LegendBuildings>>,
    // ponytail: cache key in a Local so identical selections don't respawn the
    // table every frame; it flips only on a new land or a changed building set.
    mut key: Local<Option<String>>,
    mut commands: Commands,
    lands: Query<(&LandName, &LandHasBuildings)>,
    building_of: Query<&BuildingOf>,
    kingdoms: Query<(&StringId, &KingdomHolds, Option<&KingdomSeat>, Option<&KingdomLedBy>)>,
    chars: Query<(&CharacterName, &CharacterAge)>,
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
) {
    // Nothing selected, or a selected id the world doesn't resolve to a land:
    // blank the info/buildings (actions live in their own system and resolve
    // their own selection state — they self-clear to `(none)` when unselected).
    let Some((id, land_e, (land_name, land_has_buildings))) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id).map(|e| (id.clone(), e)))
        .and_then(|(id, e)| lands.get(e).ok().map(|ld| (id, e, ld)))
    else {
        info.0.clear();
        rebuild(&mut commands, *container, &mut key, None, &[], None);
        return;
    };

    // Section 1: id, land, kingdom detail.
    let mut inf = format!("id:{id}\nname:{}", land_name.0);
    if let Some((kingdom_string_id, _, kingdom_seat, kingdom_led_by)) = kingdoms
        .iter()
        .find(|(_, kingdom_holds, _, _)| kingdom_holds.iter().any(|e| e == land_e))
    {
        inf.push_str(&format!("\nkingdom:{}", kingdom_string_id.0));
        if kingdom_seat.is_some_and(|kingdom_seat| kingdom_seat.0 == land_e) {
            inf.push_str(" (seat)");
        }
        if let Some(kingdom_led_by) = kingdom_led_by
            && let Ok((character_name, character_age)) = chars.get(kingdom_led_by.0)
        {
            let house = character_of_house
                .get(kingdom_led_by.0)
                .ok()
                .and_then(|character_of_house| {
                    houses.get(character_of_house.0).ok()
                })
                .map(|house_name| house_name.0.clone())
                .unwrap_or_default();
            inf.push_str(&format!(
                "\nruler:{} of {} ({})",
                character_name.0, house, character_age.0
            ));
        }
    }
    info.0 = inf;

    // Section 2: per-building yield and total — walk the land's building
    // instances through to their definitions for the stats.
    let (mut gold, mut levy) = (0i64, 0u64);
    let mut rows: Vec<(String, String, String)> = Vec::new();
    // The key tracks selection + the building roster on it (def ids in order);
    // it changes when the land or its building set does.
    let mut sig = String::new();
    for b_e in land_has_buildings.iter() {
        let Some(building_of) = building_of.get(b_e).ok() else {
            continue;
        };
        if !sig.is_empty() {
            sig.push(',');
        }
        sig.push_str(&building_of.0);
        let Some(d) = defs.get(&building_of.0) else {
            continue;
        };
        gold += d.gold_profit as i64 - d.gold_upkeep as i64;
        levy += d.levy as u64;
        // One gold field is ever set (profit xor upkeep), so the net is
        // whichever sign is present; omit zeroes to keep a cell clean.
        let g = if d.gold_profit > 0 {
            format!("+{}g", d.gold_profit)
        } else if d.gold_upkeep > 0 {
            format!("-{}g", d.gold_upkeep)
        } else {
            String::new()
        };
        let l = if d.levy > 0 { d.levy.to_string() } else { String::new() };
        rows.push((d.name.clone(), g, l));
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

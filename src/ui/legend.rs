//! The legend panel above the chronicle: what the map selection is.

use super::{FONT, TITLE};
use crate::app::Game;
use bevy::prelude::*;

#[derive(Component)]
pub struct Legend;

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
        p.spawn((Legend, Text::new(""), TextFont::from_font_size(FONT)));
    });
}

pub fn update(game: Res<Game>, mut legend: Single<&mut Text, With<Legend>>) {
    // Nothing selected, or a selected id the world doesn't have: blank.
    let Some(id) = game.ctx.selected_region.clone() else {
        legend.0 = String::new();
        return;
    };
    let Some(name) = game.ctx.land_name(&id) else {
        legend.0 = String::new();
        return;
    };

    let mut out = format!("id:{id}\nname:{name}");
    if let Some(k) = game.ctx.kingdom_of_land(&id) {
        out.push_str(&format!("\nkingdom:{}", k.id));
        if k.seat_land_id == id {
            out.push_str(" (seat)");
        }
        if let Some(c) = game.ctx.character_brief(&k.leader_character_id) {
            out.push_str(&format!("\nruler:{} of {} ({})", c.name, c.house_name, c.age));
        }
    }

    let built = game.ctx.buildings_in_land(&id);
    let (mut gold, mut levy) = (0i64, 0u64);
    for b in &built {
        gold += b.gold_profit as i64 - b.gold_upkeep as i64;
        levy += b.levy as u64;
        // ponytail: only the non-zero numbers, so a line reads
        // "market square +10g" not "+10g -0g 0 levy".
        out.push_str(&format!("\n- {}", b.name));
        if b.gold_profit > 0 {
            out.push_str(&format!(" +{}g", b.gold_profit));
        }
        if b.gold_upkeep > 0 {
            out.push_str(&format!(" -{}g", b.gold_upkeep));
        }
        if b.levy > 0 {
            out.push_str(&format!(" {} levy", b.levy));
        }
    }
    if !built.is_empty() {
        out.push_str(&format!("\ntotal: {gold:+}g {levy} levy"));
    }
    legend.0 = out;
}

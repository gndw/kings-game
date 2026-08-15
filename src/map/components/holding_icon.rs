//! Visual marker for a kingdom's holding (castle) on the map, plus the
//! per-land name + yield `Text2d` label. Both anchor to the same
//! land-holding point.

use super::common::UIWithKingdom;
use super::super::FONT_SIZE;
use crate::app::Game;
use crate::ecs::kingdom::{Kingdom, KingdomHold};
use crate::ecs::land::{Land, LandHasBuildings, LandHeldBy, LandHolding, LandName};
use crate::ecs::{BuildingOf, BuildingStatus, CharacterLeads, Registry};
use crate::resources::buildings::BuildingDefs;
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;

/// Marker on the entity whose world translation is the anchor for the holding-icon visual.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoldingIcon;

/// Marker on the `Text2d` entities spawned for a land's name + yield label.
#[derive(Component)]
pub struct LandLabel(pub Entity);

// Castle proportions, world units.
const TOWER_W: f32 = 8.0;
const TOWER_H: f32 = 22.0;
const SIDE_TOWER_H: f32 = 14.0;
const TOWER_SPACING: f32 = 13.0;
const GATE_W: f32 = 4.0;
const GATE_H: f32 = 6.0;
const CRENEL_W: f32 = 2.0;
const CRENEL_GAP: f32 = 2.0;
const CRENEL_DEPTH: f32 = 2.0;
const CASTLE_BROWN: Srgba = Srgba::rgb(0.59, 0.29, 0.0);
const HOLDING_LABEL_OFFSET: f32 = 6.0;
const LABEL_BORDER_OFFSET: f32 = 1.5;
const LABEL_BORDER_SHADOWS: [(f32, f32); 4] = [
    (LABEL_BORDER_OFFSET, 0.0),
    (-LABEL_BORDER_OFFSET, 0.0),
    (0.0, LABEL_BORDER_OFFSET),
    (0.0, -LABEL_BORDER_OFFSET),
];

/// Draw the castle silhouette in `color` lines at world point `at` (the bottom-centre).
pub fn draw(gizmos: &mut Gizmos, at: Vec2, color: Srgba) {
    let tower_xs = [at.x - TOWER_SPACING, at.x, at.x + TOWER_SPACING];
    let tower_heights = [SIDE_TOWER_H, TOWER_H, SIDE_TOWER_H];

    for i in 0..2 {
        let prev_right = tower_xs[i] + TOWER_W / 2.0;
        let next_left = tower_xs[i + 1] - TOWER_W / 2.0;
        let wall_w = next_left - prev_right;
        let wall_center_x = (prev_right + next_left) / 2.0;
        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(
                wall_center_x,
                at.y + SIDE_TOWER_H / 2.0,
            )),
            Vec2::new(wall_w, SIDE_TOWER_H),
            color,
        );
    }

    for (i, &tx) in tower_xs.iter().enumerate() {
        let h = tower_heights[i];
        let top = at.y + h;
        let left = tx - TOWER_W / 2.0;
        let right = tx + TOWER_W / 2.0;

        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(tx, at.y + h / 2.0)),
            Vec2::new(TOWER_W, h),
            color,
        );

        crenellations(gizmos, left, right, top, color);
    }

    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, at.y + GATE_H / 2.0)),
        Vec2::new(GATE_W, GATE_H),
        color,
    );
}

fn crenellations(gizmos: &mut Gizmos, left: f32, right: f32, top: f32, color: Srgba) {
    let pitch = CRENEL_W + CRENEL_GAP;
    let tower_w = right - left;
    let n_teeth = (tower_w / pitch).floor() as i32;
    if n_teeth == 0 {
        return;
    }

    let total_w = n_teeth as f32 * CRENEL_W + (n_teeth as f32 - 1.0) * CRENEL_GAP;
    let start = left + (tower_w - total_w) / 2.0;

    let mut path = vec![Vec2::new(left, top)];
    let mut x = start;
    for _ in 0..n_teeth {
        path.push(Vec2::new(x, top + CRENEL_DEPTH));
        path.push(Vec2::new(x + CRENEL_W, top + CRENEL_DEPTH));
        path.push(Vec2::new(x + CRENEL_W, top));
        path.push(Vec2::new(x + pitch, top));
        x += pitch;
    }
    path.push(Vec2::new(right, top));

    gizmos.linestrip_2d(path.iter().copied(), color);
}

/// Spawn one `HoldingIcon` per kingdom plus five `LandLabel` `Text2d` entities per land.
pub fn startup(
    mut commands: Commands,
    kingdoms: Query<Entity, With<Kingdom>>,
    lands: Query<(Entity, &LandName, &LandHolding), With<Land>>,
) {
    for kingdom_e in &kingdoms {
        commands.spawn((HoldingIcon, UIWithKingdom(kingdom_e), Transform::default()));
    }

    for (land_e, name, holding) in &lands {
        let x = holding.0.0 as f32;
        let y = holding.0.1 as f32 - HOLDING_LABEL_OFFSET;
        for (dx, dy) in LABEL_BORDER_SHADOWS {
            commands.spawn((
                Text2d::new(name.0.clone()),
                TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
                TextColor(Color::Srgba(css::BLACK)),
                TextLayout::new(Justify::Center, LineBreak::WordBoundary),
                Anchor::TOP_CENTER,
                LandLabel(land_e),
                Transform::from_xyz(x + dx, y + dy, 1.0),
            ));
        }
        commands.spawn((
            Text2d::new(name.0.clone()),
            TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
            TextColor(Color::Srgba(css::WHITE)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            Anchor::TOP_CENTER,
            LandLabel(land_e),
            Transform::from_xyz(x, y, 1.0),
        ));
    }
}

/// Per-frame update: position each castle at its kingdom's home land and draw
/// the castle gizmo (yellow on the selected land, brown otherwise); refresh
/// each land label's text.
pub fn update(
    mut icons: Query<(&UIWithKingdom, &mut Transform), With<HoldingIcon>>,
    kingdoms: Query<&KingdomHold>,
    lands: Query<&LandHolding>,
    land_held_by: Query<&LandHeldBy, With<Land>>,
    game: Res<Game>,
    registry: Res<Registry>,
    character_leads: Query<&CharacterLeads>,
    defs: Res<BuildingDefs>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    land_names: Query<&LandName>,
    mut labels: Query<(&LandLabel, &mut Text2d)>,
    mut gizmos: Gizmos,
) {
    let sel_land_e = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id));

    let player_kingdoms: std::collections::HashSet<bevy::ecs::entity::Entity> = game
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|pe| character_leads.get(pe).ok())
        .map(|cl| cl.kingdoms().iter().copied().collect())
        .unwrap_or_default();

    for (ui_with_kingdom, mut transform) in &mut icons {
        let Ok(kingdom_hold) = kingdoms.get(ui_with_kingdom.0) else {
            continue;
        };
        let Ok(land_holding) = lands.get(kingdom_hold.0) else {
            continue;
        };

        let pos = Vec2::new(land_holding.0.0 as f32, land_holding.0.1 as f32);
        transform.translation = pos.extend(transform.translation.z);

        let color = if sel_land_e == Some(kingdom_hold.0) {
            css::YELLOW
        } else {
            CASTLE_BROWN
        };
        draw(&mut gizmos, pos, color);
    }

    for (label, mut text) in &mut labels {
        let Ok(name) = land_names.get(label.0) else {
            continue;
        };
        let is_own = land_held_by
            .get(label.0)
            .ok()
            .map(|land_held_by| player_kingdoms.contains(&land_held_by.kingdom()))
            .unwrap_or(false);
        if is_own {
            let (gold, levy) = crate::game::yielding::sum_land_yield(
                label.0,
                &land_has_buildings,
                &building_of,
                &building_status,
                &defs,
            );
            text.0 = format!("{}\n({:+}g/m {:+})", name.0, gold, levy);
        } else {
            text.0 = name.0.clone();
        }
    }
}

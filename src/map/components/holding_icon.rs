//! Visual marker for a kingdom's holding (castle) on the map: a white-line
//! castle silhouette with three towers (centre taller than sides),
//! crenellations on every tower top, connecting walls at the side-tower
//! height, and a central gate.
//!
//! Lifecycle:
//! - [`startup`] (system) spawns one [`HoldingIcon`] per kingdom, attaching
//!   [`UIWithKingdom`](super::common::UIWithKingdom) so the per-frame
//!   [`update`] can look up the kingdom data.
//! - [`update`] (system) positions each icon at its kingdom's home land
//!   (`KingdomHold` → `LandHolding`) and draws the castle gizmo. The
//!   selected land's castle flips to yellow (the selection cue); the rest
//!   stay brown.
//!
//! Visual-only — lifecycle is event-free.

use super::common::UIWithKingdom;
use crate::app::Game;
use crate::ecs::kingdom::{Kingdom, KingdomHold};
use crate::ecs::land::LandHolding;
use crate::ecs::Registry;
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Marker on an entity whose world translation is the anchor point for
/// the holding-icon visual (the ground at the castle's base).
#[derive(Component, Debug, Clone, Copy)]
pub struct HoldingIcon;

// Castle proportions, world units. Sized to sit next to the army icon
// sword at comparable visual weight.
const TOWER_W: f32 = 8.0;
const TOWER_H: f32 = 22.0;
const SIDE_TOWER_H: f32 = 14.0;
const TOWER_SPACING: f32 = 13.0;
const GATE_W: f32 = 4.0;
const GATE_H: f32 = 6.0;
const CRENEL_W: f32 = 2.0;
const CRENEL_GAP: f32 = 2.0;
const CRENEL_DEPTH: f32 = 2.0;

/// Unselected castle colour.
const CASTLE_BROWN: Srgba = Srgba::rgb(0.59, 0.29, 0.0);

/// Draw the castle silhouette in `color` lines at world point `at`. `at`
/// is the bottom-centre of the castle (ground level).
///
/// Drawn back-to-front by z-order of gizmo draws in the frame: walls
/// first, then towers (which overlap the wall tops), then crenellations
/// and the gate last. All in the default gizmo group; the relative order
/// within a single `draw` call is what matters visually.
pub fn draw(gizmos: &mut Gizmos, at: Vec2, color: Srgba) {
    let tower_xs = [at.x - TOWER_SPACING, at.x, at.x + TOWER_SPACING];
    let tower_heights = [SIDE_TOWER_H, TOWER_H, SIDE_TOWER_H];

    // Wall sections between adjacent towers, at side-tower height.
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

    // Three towers: side towers shorter, centre taller.
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

    // Gate: small rectangle at the centre of the central tower's base.
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

    // Centre the tooth pattern in the tower's width.
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

/// Spawn one [`HoldingIcon`] per kingdom at world origin. The per-frame
/// [`update`] system positions each icon at its kingdom's home land.
pub fn startup(mut commands: Commands, kingdoms: Query<Entity, With<Kingdom>>) {
    for kingdom_e in &kingdoms {
        commands.spawn((HoldingIcon, UIWithKingdom(kingdom_e), Transform::default()));
    }
}

/// Per-frame update: position each icon at its kingdom's home land and
/// draw the castle gizmo. The selected land's castle flips to yellow as
/// the selection cue; the rest stay brown.
pub fn update(
    mut icons: Query<(&UIWithKingdom, &mut Transform), With<HoldingIcon>>,
    kingdoms: Query<&KingdomHold>,
    lands: Query<&LandHolding>,
    game: Res<Game>,
    registry: Res<Registry>,
    mut gizmos: Gizmos,
) {
    let sel_land_e = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id));

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
}

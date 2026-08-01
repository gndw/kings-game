//! The camera framed on the map and the gizmo drawing of it. The map geometry
//! itself lives in `crate::content`.

use super::flag;
use super::startup::RIGHT_BAR;
use crate::app::Game;
use crate::content::bounds;
use bevy::camera::ScalingMode;
use bevy::color::palettes::css;
use bevy::prelude::*;

const HOLDING_RADIUS: f32 = 4.0;
/// Gap between the horizontal lines that stand in for a polygon fill.
// ponytail: fixed world-space step, like everything else here — the camera
// never zooms, so it can't go coarse on screen.
const FILL_STEP: f64 = 3.0;

/// Wash a land's polygon in `color`. Gizmos draw lines only, so the fill is a
/// stack of horizontal scanlines: at each height, cross the polygon's edges and
/// join the crossings up in pairs. Handles the concave lands the map has.
fn fill(gizmos: &mut Gizmos, poly: &[(f64, f64)], color: Color) {
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &(_, y) in poly {
        lo = lo.min(y);
        hi = hi.max(y);
    }
    let mut y = lo + FILL_STEP / 2.0;
    while y < hi {
        // Edges wrap around, so an outline that doesn't repeat its first point
        // still closes. A repeated one just yields a zero-length edge.
        let mut xs: Vec<f64> = poly
            .iter()
            .zip(poly.iter().cycle().skip(1))
            .take(poly.len())
            .filter_map(|(&(xa, ya), &(xb, yb))| {
                ((ya > y) != (yb > y)).then(|| xa + (y - ya) / (yb - ya) * (xb - xa))
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        for span in xs.chunks_exact(2) {
            gizmos.line_2d(
                Vec2::new(span[0] as f32, y as f32),
                Vec2::new(span[1] as f32, y as f32),
                color,
            );
        }
        y += FILL_STEP;
    }
}

/// Camera framed on the whole map.
pub fn startup(mut commands: Commands, game: Res<Game>) {
    let (x0, x1, y0, y1) = bounds(&game.ctx.content);
    commands.spawn((
        Camera2d,
        // AutoMin keeps the whole map visible whatever the window shape, so the
        // island never distorts — the viewport maths the terminal needed is free here.
        // Widened and pushed left so the map lands beside the chronicle, not under
        // it: the camera renders the whole window, the panels just sit on top.
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: ((x1 - x0) * 1.05) as f32 / (1.0 - RIGHT_BAR),
                min_height: ((y1 - y0) * 1.05) as f32,
            },
            viewport_origin: Vec2::new((1.0 - RIGHT_BAR) / 2.0, 0.5),
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32, 0.0),
    ));
}

/// Arrow keys move the selection to the neighbouring land in that direction.
pub fn update_input(keys: Res<ButtonInput<KeyCode>>, mut game: ResMut<Game>) {
    for (key, dir) in [
        (KeyCode::ArrowLeft, (-1.0, 0.0)),
        (KeyCode::ArrowRight, (1.0, 0.0)),
        (KeyCode::ArrowUp, (0.0, 1.0)),
        (KeyCode::ArrowDown, (0.0, -1.0)),
    ] {
        if keys.just_pressed(key)
            && let Some(sel) = game.ctx.selected_region.clone()
            && let Some(next) = game.ctx.content.step(&sel, dir)
        {
            game.ctx.selected_region = Some(next);
        }
    }
}

/// World border, land outlines, holdings, and the selected land's flag.
pub fn update_draw(mut gizmos: Gizmos, game: Res<Game>, time: Res<Time>) {
    let b = &game.ctx.content.border;
    gizmos.rect_2d(
        Isometry2d::from_xy(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
        Vec2::new((b.x1 - b.x0) as f32, (b.y1 - b.y0) as f32),
        css::BLUE,
    );

    let sel = game.ctx.selected_region.as_deref();
    let own: &[String] = game
        .ctx
        .content
        .kingdom_led_by(&game.ctx.player_character_id)
        .map_or(&[], |k| k.land_ids.as_slice());
    // Selected land last, so it draws over its neighbours.
    let order = game
        .ctx
        .content
        .lands
        .iter()
        .filter(|s| Some(s.id.as_str()) != sel)
        .chain(
            game.ctx
                .content
                .lands
                .iter()
                .filter(|s| Some(s.id.as_str()) == sel),
        );
    for land in order {
        let is_sel = Some(land.id.as_str()) == sel;
        let (outline, holder) = if is_sel {
            (css::YELLOW, css::YELLOW)
        } else {
            (css::WHITE, Srgba::rgb(0.59, 0.29, 0.0))
        };
        if own.contains(&land.id) {
            fill(&mut gizmos, &land.borders, css::GREEN.with_alpha(0.1).into());
        }
        gizmos.linestrip_2d(
            land.borders
                .iter()
                .map(|&(x, y)| Vec2::new(x as f32, y as f32)),
            outline,
        );
        let holding = Vec2::new(land.holding.0 as f32, land.holding.1 as f32);
        gizmos
            .circle_2d(
                Isometry2d::from_translation(holding),
                HOLDING_RADIUS,
                holder,
            )
            .resolution(24);
        if is_sel {
            flag::draw(&mut gizmos, holding, time.elapsed_secs());
        }
    }
}

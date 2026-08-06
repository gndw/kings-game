//! The camera: spawn, framing, and the tween between zoomed/unzoomed views.
//! Map drawing lives in `super::map`; this module only knows about the
//! projection and where the camera is pointed.

use super::startup::RIGHT_BAR;
use crate::app::Game;
use crate::ecs::{LandBorders, LandHolding, Registry};
use crate::resources::border::Border;
use bevy::camera::ScalingMode;
use bevy::prelude::*;

/// Padding around the selected land's bbox when zoomed in, in world units per
/// land-unit. 3.0 = 200% of the bbox in each axis, i.e. the land takes the
/// middle third of the visible area.
const ZOOM_MARGIN: f64 = 3.0;
/// The 0.7 default scale on the orthographic projection (30% zoom-in over a
/// 1:1 view). Kept consistent across default and zoomed views so the
/// transition doesn't pop.
const CAMERA_SCALE: f32 = 0.7;
/// Seconds to interpolate between camera views. Short enough to feel snappy
/// on zoom toggle, long enough that a pan across the map doesn't strobe.
const TRANSITION_DURATION: f32 = 0.2;

/// The view the camera is currently rendering — kept in sync with the
/// `Projection`/`Transform` after each [`update_camera`] frame. Doubles as
/// the "where are we now" source for re-tweening: when the destination moves
/// mid-transition, [`update_camera`] copies this into the tween's `from` so
/// the new transition starts from the actual on-screen position, not the
/// tween's original start.
#[derive(Component, Clone, Copy, Default, PartialEq)]
pub struct CameraView {
    pub translation: Vec2,
    pub min_w: f32,
    pub min_h: f32,
}

/// In-flight camera tween. `from` is the view at the moment the destination
/// last changed; `to` is the destination; `t` is normalised progress
/// (0 = at `from`, 1 = at `to`).
#[derive(Component, Clone, Copy, Default)]
pub struct CameraTween {
    pub from: CameraView,
    pub to: CameraView,
    pub t: f32,
}

/// Smoothstep ease — gentle start and end, linear-ish middle. Feels right
/// for camera moves: avoids the jump-cut of linear lerp and the overshoot of
/// back/elastic.
fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Camera framed on the whole map. `update_camera` rewrites the projection
/// every frame to follow `Game::zoomed`; the tween starts settled so the
/// first frame doesn't kick an unintended transition toward the default view.
///
/// Pan/zoom hooks: pan = `Transform::translation`, zoom =
/// `OrthographicProjection::scale` (currently constant at `CAMERA_SCALE`,
/// 30% zoom-in over a 1:1 view). The `viewport_origin` shift centres the
/// rendered area on the left `(1 - RIGHT_BAR)` slice of the window so the
/// map lands beside the right-hand UI column instead of under it.
pub fn startup(mut commands: Commands, border: Res<Border>) {
    let (x0, x1, y0, y1) = border.bounds();
    let default_view = CameraView {
        translation: Vec2::new(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32),
        min_w: ((x1 - x0) * 1.05) as f32 / (1.0 - RIGHT_BAR),
        min_h: ((y1 - y0) * 1.05) as f32,
    };
    commands.spawn((
        Camera2d,
        // AutoMin keeps the whole map visible whatever the window shape, so
        // the island never distorts — the viewport maths the terminal needed
        // is free here. Widened and pushed left so the map lands beside the
        // chronicle, not under it: the camera renders the whole window, the
        // panels just sit on top. `update_camera` rewrites `scaling_mode` and
        // `transform` every frame to follow `Game::zoomed`.
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: default_view.min_w,
                min_height: default_view.min_h,
            },
            viewport_origin: Vec2::new((1.0 - RIGHT_BAR) / 2.0, 0.5),
            scale: CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(default_view.translation.x, default_view.translation.y, 0.0),
        default_view,
        CameraTween {
            from: default_view,
            to: default_view,
            t: 1.0,
        },
    ));
}

/// Compute the camera target for the current `(Game::zoomed, selection)`
/// state. Returns the default map view when unzoomed or when the selection
/// can't be resolved; otherwise the selected land's bbox + `ZOOM_MARGIN`,
/// centred on the bbox (polygons can be off-centre, so the holding point
/// isn't always the visual centre).
fn compute_target(
    game: &Game,
    border: &Border,
    registry: &Registry,
    lands: &Query<(&LandBorders, &LandHolding)>,
) -> CameraView {
    let (x0, x1, y0, y1) = border.bounds();
    let mut target = CameraView {
        translation: Vec2::new(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32),
        min_w: ((x1 - x0) * 1.05) as f32 / (1.0 - RIGHT_BAR),
        min_h: ((y1 - y0) * 1.05) as f32,
    };
    if game.zoomed
        && let Some(sel) = game.ctx.selected_land_id.as_deref()
        && let Some(land_e) = registry.get(sel)
        && let Ok((borders, _holding)) = lands.get(land_e)
    {
        let (mut lx0, mut lx1, mut ly0, mut ly1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for &(x, y) in &borders.0 {
            lx0 = lx0.min(x);
            lx1 = lx1.max(x);
            ly0 = ly0.min(y);
            ly1 = ly1.max(y);
        }
        target.translation = Vec2::new(
            ((lx0 + lx1) / 2.0) as f32,
            ((ly0 + ly1) / 2.0) as f32,
        );
        target.min_w = ((lx1 - lx0) * ZOOM_MARGIN) as f32 / (1.0 - RIGHT_BAR);
        target.min_h = ((ly1 - ly0) * ZOOM_MARGIN) as f32;
    }
    target
}

/// Drive the camera from `Game::zoomed` and the current selection. Re-tweens
/// whenever the destination moves (zoom toggle, or selection change while
/// zoomed); otherwise just advances the in-flight tween. The new `from` is
/// the camera's current rendered view, so a re-target mid-transition stays
/// smooth instead of snapping back to the previous `from`.
pub fn update_camera(
    game: Res<Game>,
    border: Res<Border>,
    registry: Res<Registry>,
    lands: Query<(&LandBorders, &LandHolding)>,
    time: Res<Time>,
    mut camera: Single<
        (&mut Projection, &mut Transform, &mut CameraView, &mut CameraTween),
        With<Camera2d>,
    >,
) {
    let (ref mut proj, ref mut transform, ref mut view, ref mut tween) = *camera;
    let target = compute_target(&game, &border, &registry, &lands);
    if tween.to != target {
        tween.from = **view;
        tween.to = target;
        tween.t = 0.0;
    }
    tween.t = (tween.t + time.delta_secs() / TRANSITION_DURATION).min(1.0);
    let eased = ease_in_out(tween.t);
    let interp = CameraView {
        translation: tween.from.translation.lerp(tween.to.translation, eased),
        min_w: tween.from.min_w + (tween.to.min_w - tween.from.min_w) * eased,
        min_h: tween.from.min_h + (tween.to.min_h - tween.from.min_h) * eased,
    };
    let Projection::Orthographic(ref mut ortho) = **proj else {
        return;
    };
    ortho.scaling_mode = ScalingMode::AutoMin {
        min_width: interp.min_w,
        min_height: interp.min_h,
    };
    ortho.scale = CAMERA_SCALE;
    ortho.viewport_origin = Vec2::new((1.0 - RIGHT_BAR) / 2.0, 0.5);
    transform.translation = Vec3::new(interp.translation.x, interp.translation.y, 0.0);
    **view = interp;
}
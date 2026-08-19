//! The camera: spawn, framing, and the tween between zoomed/unzoomed views.

use crate::app::Game;
use crate::ecs::{LandBorders, LandHolding, Registry};
use crate::resources::border::Border;
use crate::ui::kingdom::KingdomUiContext;
use bevy::camera::ScalingMode;
use bevy::prelude::*;

/// Padding around the selected land's bbox when zoomed in, in world units per
/// land-unit.
const ZOOM_MARGIN: f64 = 3.0;
/// Floor on the zoomed view's `min_w`/`min_h` so the view stays readable on small lands.
const MIN_ZOOM: f32 = 2000.0;
/// 30% zoom-in over a 1:1 view. Kept consistent across default and zoomed views so the transition doesn't pop.
const CAMERA_SCALE: f32 = 0.7;
/// Seconds to interpolate between camera views.
const TRANSITION_DURATION: f32 = 0.2;
/// Width of the right-docked kingdom panel as a fraction of the window.
/// Must match the `width: percent(...)` in
/// [`crate::ui::kingdom::startup`]. Used to shift the camera so map content
/// stays centered in the *open* view (i.e. excluding the panel area).
const KINGDOM_PANEL_WIDTH: f32 = 0.35;

/// The view the camera is currently rendering. Doubles as the source for re-tweening.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct CameraView {
    pub translation: Vec2,
    pub min_w: f32,
    pub min_h: f32,
    /// Viewport x-origin where this view's translation should land, in
    /// normalised viewport coords (0 = left edge, 1 = right edge).
    /// Default `0.5` centers the map; a right-side panel nudges it toward
    /// `0` so the map stays centered in the *visible* (panel-excluded) area.
    pub viewport_origin_x: f32,
}

impl Default for CameraView {
    fn default() -> Self {
        Self {
            translation: Vec2::ZERO,
            min_w: 0.0,
            min_h: 0.0,
            viewport_origin_x: 0.5,
        }
    }
}

/// In-flight camera tween. `from` is the view at the moment the destination
/// last changed; `to` is the destination; `t` is normalised progress.
#[derive(Component, Clone, Copy, Default)]
pub struct CameraTween {
    pub from: CameraView,
    pub to: CameraView,
    pub t: f32,
}

/// Smoothstep ease — gentle start and end.
fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Camera framed on the whole map. `update_camera` rewrites the projection
/// every frame to follow `Game::zoomed`, the selection, and the kingdom
/// panel state; the tween starts settled so the first frame doesn't kick
/// an unintended transition.
pub fn startup(mut commands: Commands, border: Res<Border>) {
    let (x0, x1, y0, y1) = border.bounds();
    let default_view = CameraView {
        translation: Vec2::new(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32),
        min_w: ((x1 - x0) * 1.05) as f32,
        min_h: ((y1 - y0) * 1.05) as f32,
        viewport_origin_x: 0.5,
    };
    commands.spawn((
        Camera2d,
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: default_view.min_w,
                min_height: default_view.min_h,
            },
            viewport_origin: Vec2::new(0.5, 0.5),
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

/// The camera target for the current `(Game::zoomed, selection, panel)` state.
fn compute_target(
    game: &Game,
    border: &Border,
    registry: &Registry,
    lands: &Query<(&LandBorders, &LandHolding)>,
    kingdom_ui: &KingdomUiContext,
) -> CameraView {
    let (x0, x1, y0, y1) = border.bounds();
    let panel_open = kingdom_ui.pinned_kingdom_id.is_some();
    // Right-docked panel takes the right `KINGDOM_PANEL_WIDTH` of the window,
    // so the visible region runs from x=0 to x=(1 - KINGDOM_PANEL_WIDTH).
    // Its midpoint is at `(1 - KINGDOM_PANEL_WIDTH) / 2` viewport-x, shifted
    // left from the default centre by half the panel width.
    let viewport_origin_x = if panel_open {
        (1.0 - KINGDOM_PANEL_WIDTH) / 2.0
    } else {
        0.5
    };
    let mut target = CameraView {
        translation: Vec2::new(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32),
        min_w: ((x1 - x0) * 1.05) as f32,
        min_h: ((y1 - y0) * 1.05) as f32,
        viewport_origin_x,
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
        target.min_w = (((lx1 - lx0) * ZOOM_MARGIN) as f32).max(MIN_ZOOM);
        target.min_h = (((ly1 - ly0) * ZOOM_MARGIN) as f32).max(MIN_ZOOM);
    }
    target
}

/// Drive the camera from `Game::zoomed`, the current selection, and the
/// kingdom panel state. Re-tweens whenever the destination moves; otherwise
/// just advances the tween.
pub fn update_camera(
    game: Res<Game>,
    border: Res<Border>,
    registry: Res<Registry>,
    lands: Query<(&LandBorders, &LandHolding)>,
    kingdom_ui: Res<KingdomUiContext>,
    time: Res<Time>,
    mut camera: Single<
        (&mut Projection, &mut Transform, &mut CameraView, &mut CameraTween),
        With<Camera2d>,
    >,
) {
    let (ref mut proj, ref mut transform, ref mut view, ref mut tween) = *camera;
    let target = compute_target(&game, &border, &registry, &lands, &kingdom_ui);
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
        viewport_origin_x: tween.from.viewport_origin_x
            + (tween.to.viewport_origin_x - tween.from.viewport_origin_x) * eased,
    };
    let Projection::Orthographic(ref mut ortho) = **proj else {
        return;
    };
    ortho.scaling_mode = ScalingMode::AutoMin {
        min_width: interp.min_w,
        min_height: interp.min_h,
    };
    ortho.scale = CAMERA_SCALE;
    ortho.viewport_origin = Vec2::new(interp.viewport_origin_x, 0.5);
    transform.translation = Vec3::new(interp.translation.x, interp.translation.y, 0.0);
    **view = interp;
}

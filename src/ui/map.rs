//! The camera framed on the map and the gizmo drawing of it. The map geometry
//! itself lives in the entity world; see `crate::ecs::Land`.

use super::flag;
use super::startup::RIGHT_BAR;
use crate::app::Game;
use crate::ecs::{
    BuildingOf, CharacterLeads, KingdomHolds, LandBorders, LandHasBuildings, LandHolding,
    LandName, Registry, StringId,
};
use crate::resources::border::Border;
use crate::resources::buildings::BuildingDefs;
use bevy::camera::ScalingMode;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashSet;

const HOLDING_RADIUS: f32 = 12.0;
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

/// Marker on the `Text2d` entity we spawn in [`startup`] for a land's name +
/// yield, so [`update_draw`] can find the label for a given land and refresh
/// the yield line.
#[derive(Component)]
pub struct LandLabel(pub Entity);

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

/// Gap between the horizontal lines that stand in for a polygon fill.
// ponytail: fixed world-space step — re-derive from the camera's current
// visible-size ratio if zoom gets coarse enough to show gaps.
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

/// Camera framed on the whole map; also spawns one `Text2d` label per land,
/// just below the holding circle.
///
/// Pan/zoom hook in here: change `Transform::translation` to pan,
/// `OrthographicProjection::scale` to zoom (Bevy 0.19 multiplies the projection
/// area by `scale`, so the visible world shrinks/grows around the projection's
/// centre). The `viewport_origin` shift centres the rendered area on the left
/// `(1 - RIGHT_BAR)` slice of the window so the map lands beside the
/// right-hand UI column instead of under it.
pub fn startup(
    mut commands: Commands,
    border: Res<Border>,
    // populate has already run by the time Startup schedules, so the land
    // entities exist and this query resolves.
    lands: Query<(Entity, &LandName, &LandHolding)>,
) {
    let (x0, x1, y0, y1) = border.bounds();
    let default_view = CameraView {
        translation: Vec2::new(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32),
        min_w: ((x1 - x0) * 1.05) as f32 / (1.0 - RIGHT_BAR),
        min_h: ((y1 - y0) * 1.05) as f32,
    };
    commands.spawn((
        Camera2d,
        // AutoMin keeps the whole map visible whatever the window shape, so the
        // island never distorts — the viewport maths the terminal needed is free
        // here. Widened and pushed left so the map lands beside the chronicle,
        // not under it: the camera renders the whole window, the panels just sit
        // on top. `update_camera` rewrites `scaling_mode` and `transform` every
        // frame to follow `Game::zoomed`.
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
        // Tween starts settled (t = 1) so the first `update_camera` frame
        // doesn't kick an unintended transition toward the default view.
        default_view,
        CameraTween {
            from: default_view,
            to: default_view,
            t: 1.0,
        },
    ));
    // One world-space label per land, just below the holding circle. Spawned
    // once in Startup so update_draw stays gizmo-only and the labels don't get
    // respawned every frame. The name is the first line; update_draw appends
    // the yield line below it.
    for (land_e, name, holding) in &lands {
        commands.spawn((
            Text2d::new(name.0.clone()),
            TextFont::from_font_size(18.0),
            TextColor(Color::Srgba(css::WHITE)),
            // Centre each line within its own bounding box so the two lines
            // stack as a centred column under the holding, not a ragged
            // left-aligned one.
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            Anchor::TOP_CENTER,
            LandLabel(land_e),
            Transform::from_xyz(
                holding.0.0 as f32,
                holding.0.1 as f32 - HOLDING_RADIUS - 4.0,
                1.0,
            ),
        ));
    }
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

/// Arrow keys move the selection to the neighbouring land in that direction.
/// Exclusive: selection stepping reads many lands and writes the player's
/// selection, all through the one [`World`].
pub fn update_input(world: &mut World) {
    // The command palette owns the arrows while open; don't move the selection.
    if world.resource::<crate::ui::command_menu::CommandMenu>().open {
        return;
    }
    let dir = [
        (KeyCode::ArrowLeft, (-1.0, 0.0)),
        (KeyCode::ArrowRight, (1.0, 0.0)),
        (KeyCode::ArrowUp, (0.0, 1.0)),
        (KeyCode::ArrowDown, (0.0, -1.0)),
    ]
    .into_iter()
    .find_map(|(k, d)| {
        world
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(k)
            .then_some(d)
    });
    let Some(dir) = dir else {
        return;
    };
    let sel = world.resource::<Game>().ctx.selected_land_id.clone();
    let Some(sel) = sel else {
        return;
    };
    if let Some(next) = crate::ctx::step(world, &sel, dir) {
        world.resource_mut::<Game>().ctx.selected_land_id = Some(next);
    }
}

/// Sum a single land's buildings into `(gold, levy)`. The same
/// `gold_profit - gold_upkeep` and `levy` walk as
/// [`sum_kingdom_yield`](crate::updates::yields::sum_kingdom_yield) but
/// scoped to one land, so the map label can show the per-land total.
fn sum_land_yield(
    land_e: Entity,
    land_has_buildings: &Query<&LandHasBuildings>,
    building_of: &Query<&BuildingOf>,
    defs: &BuildingDefs,
) -> (i64, u64) {
    let Ok(land_has_buildings) = land_has_buildings.get(land_e) else {
        return (0, 0);
    };
    let (mut gold, mut levy) = (0i64, 0u64);
    for b_e in land_has_buildings.iter() {
        let Ok(building_of) = building_of.get(b_e) else {
            continue;
        };
        if let Some(d) = defs.get(&building_of.0) {
            gold += d.gold_profit as i64 - d.gold_upkeep as i64;
            levy += d.levy as u64;
        }
    }
    (gold, levy)
}

/// World border, land outlines, holdings, and the selected land's flag.
pub fn update_draw(
    mut gizmos: Gizmos,
    game: Res<Game>,
    registry: Res<Registry>,
    border: Res<Border>,
    time: Res<Time>,
    defs: Res<BuildingDefs>,
    character_leads: Query<&CharacterLeads>,
    kingdom_holds: Query<&KingdomHolds>,
    lands: Query<(&StringId, &LandBorders, &LandHolding)>,
    string_ids: Query<&StringId>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    land_names: Query<&LandName>,
    mut labels: Query<(&LandLabel, &mut Text2d)>,
) {
    let b = &*border;
    gizmos.rect_2d(
        Isometry2d::from_xy(((b.x0 + b.x1) / 2.0) as f32, ((b.y1 + b.y0) / 2.0) as f32),
        Vec2::new((b.x1 - b.x0) as f32, (b.y1 - b.y0) as f32),
        css::BLUE,
    );
    // Sea wash inside the border, drawn before the lands so polygons cover it.
    fill(
        &mut gizmos,
        &[(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)],
        css::BLUE.with_alpha(0.01).into(),
    );

    let sel = game.ctx.selected_land_id.as_deref();
    // The player's own holdings, via the reverse CharacterLeads link.
    let own: HashSet<String> = registry
        .get(&game.ctx.player_character_id)
        .and_then(|pe| character_leads.get(pe).ok())
        .and_then(|character_leads| kingdom_holds.get(character_leads.kingdom()).ok())
        .map(|kingdom_holds| {
            kingdom_holds
                .iter()
                .filter_map(|le| string_ids.get(le).ok().map(|string_id| string_id.0.clone()))
                .collect()
        })
        .unwrap_or_default();

    // Lands in spawn order: one archetype, so `Query` yields content order.
    let lands_vec: Vec<(String, Vec<(f64, f64)>, (f64, f64))> = lands
        .iter()
        .map(|(string_id, land_borders, land_holding)| {
            (string_id.0.clone(), land_borders.0.clone(), land_holding.0)
        })
        .collect();
    // Selected land last, so it draws over its neighbours.
    let order = lands_vec
        .iter()
        .filter(|l| Some(l.0.as_str()) != sel)
        .chain(lands_vec.iter().filter(|l| Some(l.0.as_str()) == sel));
    for land in order {
        let is_sel = Some(land.0.as_str()) == sel;
        let (outline, holder) = if is_sel {
            (css::YELLOW, css::YELLOW)
        } else {
            (css::WHITE, Srgba::rgb(0.59, 0.29, 0.0))
        };
        let land_color = if own.contains(&land.0) {
            Srgba::rgb(0.012, 0.435, 0.165).with_alpha(0.1).into()
        } else {
            Srgba::rgb(0.322, 0.208, 0.165).with_alpha(0.1).into()
        };
        fill(&mut gizmos, &land.1, land_color);
        gizmos.linestrip_2d(
            land.1.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)),
            outline,
        );
        let holding = Vec2::new(land.2.0 as f32, land.2.1 as f32);
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

    // Refresh each land label's second line (per-land total yield). The name
    // was baked in at startup; the yield only changes on construct/destroy,
    // but a per-frame walk is cheap and keeps the code branch-free.
    for (label, mut text) in &mut labels {
        let name = land_names.get(label.0).map(|n| n.0.as_str()).unwrap_or("");
        let (gold, levy) = sum_land_yield(label.0, &land_has_buildings, &building_of, &defs);
        text.0 = format!("{name}\n({gold:+}g/m {levy:+})");
    }
}

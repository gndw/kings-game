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

/// Marker on the `Text2d` entity we spawn in [`startup`] for a land's name +
/// yield, so [`update_draw`] can find the label for a given land and refresh
/// the yield line.
#[derive(Component)]
pub struct LandLabel(pub Entity);
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
    commands.spawn((
        Camera2d,
        // AutoMin keeps the whole map visible whatever the window shape, so the
        // island never distorts — the viewport maths the terminal needed is free
        // here. Widened and pushed left so the map lands beside the chronicle,
        // not under it: the camera renders the whole window, the panels just sit
        // on top.
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: ((x1 - x0) * 1.05) as f32 / (1.0 - RIGHT_BAR),
                min_height: ((y1 - y0) * 1.05) as f32,
            },
            viewport_origin: Vec2::new((1.0 - RIGHT_BAR) / 2.0, 0.5),
            // 30% zoom-in (objects appear ~43% bigger). Pan/zoom wiring later
            // mutates this instead of `Transform::translation`.
            scale: 0.7,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32, 0.0),
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

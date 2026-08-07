//! Gizmo drawing of the map: world border, land outlines + fills, holdings,
//! the selected land's flag, and the per-land yield labels. The camera
//! itself lives in `super::camera`; the map geometry lives in the entity
//! world (see `crate::ecs::Land`).

use super::{flag, FONT_SIZE};
use crate::app::Game;
use crate::ecs::{
    BuildingOf, BuildingStatus, CharacterLeads, KingdomHold, LandBorders, LandHasBuildings,
    LandHolding, LandName, Registry, StringId,
};
use crate::resources::border::Border;
use crate::resources::buildings::BuildingDefs;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashSet;

const HOLDING_RADIUS: f32 = 12.0;
/// World-space offset for the per-label black outline. At the camera's 0.7
/// scale this is roughly a 1px border, just enough to lift the white text off
/// the varied land fills without overpowering the names.
const LABEL_BORDER_OFFSET: f32 = 1.5;
/// Black-text offsets that form a four-direction outline around each label.
/// `Text2d` has no built-in outline; the trick is to spawn one black copy at
/// each cardinal direction behind the main white text.
const LABEL_BORDER_SHADOWS: [(f32, f32); 4] = [
    (LABEL_BORDER_OFFSET, 0.0),
    (-LABEL_BORDER_OFFSET, 0.0),
    (0.0, LABEL_BORDER_OFFSET),
    (0.0, -LABEL_BORDER_OFFSET),
];

/// Gizmo config group dedicated to the per-land polygon outline. Uses a
/// thinner 1.0px stroke (vs the default 2.0px) so the borders read as refined
/// edges rather than chunky ones, while the world border, fill, holding ring,
/// and flag keep the default width. Registered once in `main` via
/// `AppGizmoBuilder::insert_gizmo_config` because `linestrip_2d` has no
/// per-call width in Bevy 0.19.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct LandBorderGizmoConfigGroup;

/// Marker on the `Text2d` entity we spawn in [`startup`] for a land's name +
/// yield, so [`update_draw`] can find the label for a given land and refresh
/// the yield line.
#[derive(Component)]
pub struct LandLabel(pub Entity);

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

/// Spawn one `Text2d` label per land, just below the holding circle.
/// Spawned once in Startup so `update_draw` stays gizmo-only and the labels
/// don't get respawned every frame. The name is the first line;
/// `update_draw` appends the yield line below it. The camera itself is
/// spawned by `super::camera::startup`.
pub fn startup(
    mut commands: Commands,
    // populate has already run by the time Startup schedules, so the land
    // entities exist and this query resolves.
    lands: Query<(Entity, &LandName, &LandHolding)>,
) {
    for (land_e, name, holding) in &lands {
        let x = holding.0.0 as f32;
        let y = holding.0.1 as f32 - HOLDING_RADIUS - 4.0;
        // Black outline: four black copies of the text at cardinal offsets
        // behind the main white text. `Text2dShadow` is a single drop shadow,
        // not a real outline, so the border is faked with sibling entities.
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
        // Main label on top of the outline.
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

/// World border, land outlines, holdings, and the selected land's flag.
pub fn update_draw(
    mut gizmos: Gizmos,
    // ponytail: separate `Gizmos<LandBorderGizmoConfigGroup>` so the per-land
    // polygon outline uses the 1.0px stroke configured on that group; the
    // world border, fill, holding ring, and flag stay on the default 2.0px.
    mut land_border_gizmos: Gizmos<LandBorderGizmoConfigGroup>,
    game: Res<Game>,
    registry: Res<Registry>,
    border: Res<Border>,
    time: Res<Time>,
    defs: Res<BuildingDefs>,
    character_leads: Query<&CharacterLeads>,
    kingdom_holds: Query<&KingdomHold>,
    lands: Query<(&StringId, &LandBorders, &LandHolding)>,
    string_ids: Query<&StringId>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
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
        .and_then(|kingdom_hold| string_ids.get(kingdom_hold.0).ok().map(|string_id| string_id.0.clone()))
        .map(|id| HashSet::from([id]))
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
        // Unselected lands: dark brown outline. Selected lands keep the
        // yellow outline as the selection cue (the holding circle is yellow
        // too, and the land draws last so it covers its neighbours).
        let (outline, holder) = if is_sel {
            (css::YELLOW, css::YELLOW)
        } else {
            (Srgba::rgb(0.36, 0.22, 0.12), Srgba::rgb(0.59, 0.29, 0.0))
        };
        let land_color = if own.contains(&land.0) {
            Srgba::rgb(0.012, 0.435, 0.165).with_alpha(0.1).into()
        } else {
            Srgba::rgb(0.322, 0.208, 0.165).with_alpha(0.1).into()
        };
        fill(&mut gizmos, &land.1, land_color);
        land_border_gizmos.linestrip_2d(
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
        let (gold, levy) = crate::game::yields::sum_land_yield(
            label.0,
            &land_has_buildings,
            &building_of,
            &building_status,
            &defs,
        );
        text.0 = format!("{name}\n({gold:+}g/m {levy:+})");
    }
}

//! Visual marker for a land on the map: the polygon outline + scanline
//! fill, plus the `Text2d` name + yield label that sits just below the
//! holding point. One [`LandGraphic`] is spawned per land at startup (via
//! [`startup`]); the per-frame [`update`] walks every `LandGraphic`, reads
//! the back-ref'd land's `LandBorders` + `StringId`, and draws the polygon
//! — brown outline normally, yellow when selected; brown fill normally,
//! green-tinted when the player owns the land. The label is refreshed each
//! frame so the yield line tracks construct/destroy.
//!
//! Mirrors the `holding_icon` lifecycle pattern (one startup-spawned icon
//! per entity, one per-frame update).
//!
//! Visual-only — lifecycle is event-free.

use super::common::{fill, UIWithLand};
use super::super::FONT_SIZE;
use crate::app::Game;
use crate::ecs::land::{Land, LandBorders, LandHasBuildings, LandHolding, LandName};
use crate::ecs::{BuildingOf, BuildingStatus, CharacterLeads, KingdomHold, Registry, StringId};
use crate::resources::buildings::BuildingDefs;
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashSet;

/// Marker on an entity that drives the per-land outline + fill draw.
#[derive(Component, Debug, Clone, Copy)]
pub struct LandGraphic;

/// Marker on the `Text2d` entities spawned for a land's name + yield label,
/// so [`update`] can find them and refresh the yield line each frame.
#[derive(Component)]
pub struct LandLabel(pub Entity);

/// Gizmo config group dedicated to the per-land polygon outline. Uses a
/// thinner 1.0px stroke (vs the default 2.0px) so the borders read as
/// refined edges rather than chunky ones. Registered once in `main` via
/// `App::insert_gizmo_config` because `linestrip_2d` has no per-call width
/// in Bevy 0.19.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct LandBorderGizmoConfigGroup;

/// Gap between the holding's ground point and the per-land label's top
/// edge. The castle icon's base sits on the holding point, so this is just
/// a small pad below the gate.
const HOLDING_LABEL_OFFSET: f32 = 6.0;
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

/// Spawn one [`LandGraphic`] per land (for the polygon draw), plus five
/// [`LandLabel`] entities per land (one main white label + four black
/// shadow siblings forming a 1px outline). The per-frame [`update`] system
/// refreshes the label yield and draws the polygon.
pub fn startup(
    mut commands: Commands,
    // populate has already run by the time Startup schedules, so the land
    // entities exist and this query resolves.
    lands: Query<(Entity, &LandName, &LandHolding), With<Land>>,
) {
    for (land_e, name, holding) in &lands {
        commands.spawn((LandGraphic, UIWithLand(land_e)));

        let x = holding.0.0 as f32;
        let y = holding.0.1 as f32 - HOLDING_LABEL_OFFSET;
        // Black outline: four black copies of the text at cardinal offsets
        // behind the main white text. `Text2dShadow` is a single drop
        // shadow, not a real outline, so the border is faked with sibling
        // entities.
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

/// Per-frame land outline + fill (brown normally, yellow when selected;
/// brown fill, green-tinted when player-owned), plus the yield-line refresh
/// for each land's `Text2d` label.
pub fn update(
    icons: Query<&UIWithLand, With<LandGraphic>>,
    lands: Query<&LandBorders, With<Land>>,
    lands_id: Query<&StringId, With<Land>>,
    mut land_border_gizmos: Gizmos<LandBorderGizmoConfigGroup>,
    mut gizmos: Gizmos,
    game: Res<Game>,
    registry: Res<Registry>,
    character_leads: Query<&CharacterLeads>,
    kingdom_holds: Query<&KingdomHold>,
    defs: Res<BuildingDefs>,
    land_has_buildings: Query<&LandHasBuildings>,
    building_of: Query<&BuildingOf>,
    building_status: Query<&BuildingStatus>,
    land_names: Query<&LandName>,
    mut labels: Query<(&LandLabel, &mut Text2d)>,
) {
    let sel = game.ctx.selected_land_id.as_deref();
    // Player's own lands, via the reverse CharacterLeads → KingdomHold chain.
    let own: HashSet<String> = registry
        .get(&game.ctx.player_character_id)
        .and_then(|pe| character_leads.get(pe).ok())
        .and_then(|character_leads| kingdom_holds.get(character_leads.kingdom()).ok())
        .and_then(|kingdom_hold| {
            lands_id
                .get(kingdom_hold.0)
                .ok()
                .map(|string_id| string_id.0.clone())
        })
        .map(|id| HashSet::from([id]))
        .unwrap_or_default();

    // Ponytail: collect to a Vec so we can sort the selected land to the
    // back of the draw order — matches the old "filter unselected, chain
    // selected" ordering in `ui::map::update_draw`, so a selected land's
    // outline draws on top of its neighbours instead of in archetype
    // order. Allocation is bounded by the land count (small) and happens
    // once per frame.
    let mut ordered: Vec<Entity> = icons.iter().map(|ui| ui.0).collect();
    // Key is `true` for the selected land, `false` otherwise — so the
    // selected land sorts to the back and draws over its neighbours.
    ordered.sort_by_key(|&ui_with_land| {
        lands_id
            .get(ui_with_land)
            .map(|sid| Some(sid.0.as_str()) == sel)
            .unwrap_or(false)
    });

    for ui_with_land in ordered {
        let Ok(borders) = lands.get(ui_with_land) else {
            continue;
        };
        let Ok(string_id) = lands_id.get(ui_with_land) else {
            continue;
        };
        let land_id = string_id.0.as_str();
        let is_sel = Some(land_id) == sel;
        let is_own = own.contains(land_id);

        let outline = if is_sel {
            css::YELLOW
        } else {
            Srgba::rgb(0.36, 0.22, 0.12)
        };
        let land_color = if is_own {
            Srgba::rgb(0.012, 0.435, 0.165).with_alpha(0.1).into()
        } else {
            Srgba::rgb(0.322, 0.208, 0.165).with_alpha(0.1).into()
        };

        fill(&mut gizmos, &borders.0, land_color);
        land_border_gizmos.linestrip_2d(
            borders.0.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)),
            outline,
        );
    }

    // Refresh each land label's yield line. The name was baked in at
    // startup; the yield only changes on construct/destroy, but a per-frame
    // walk is cheap and keeps the code branch-free.
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

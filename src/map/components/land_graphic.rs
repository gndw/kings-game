//! Visual marker for a land on the map: the polygon outline + scanline fill.
//! One [`LandGraphic`] is spawned per land at startup (via [`startup`]); the
//! per-frame [`update`] walks every `LandGraphic`, reads the back-ref'd
//! land's `LandBorders` + `StringId`, and draws the polygon — brown outline
//! normally, yellow when selected; brown fill normally, green-tinted when
//! the player owns the land.
//!
//! The name + yield `Text2d` label that sits just below the holding point
//! lives in `holding_icon` (the castle and the label are both anchored to
//! the same land-holding point, so they share a module). See
//! [`crate::map::components::holding_icon`].
//!
//! Mirrors the `holding_icon` lifecycle pattern (one startup-spawned icon
//! per entity, one per-frame update).
//!
//! Visual-only — lifecycle is event-free.

use super::common::{fill, UIWithLand};
use crate::app::Game;
use crate::ecs::land::{Land, LandBorders};
use crate::ecs::{CharacterLeads, KingdomHold, Registry, StringId};
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use std::collections::HashSet;

/// Marker on an entity that drives the per-land outline + fill draw.
#[derive(Component, Debug, Clone, Copy)]
pub struct LandGraphic;

/// Gizmo config group dedicated to the per-land polygon outline. Uses a
/// thinner 1.0px stroke (vs the default 2.0px) so the borders read as
/// refined edges rather than chunky ones. Registered once in `main` via
/// `App::insert_gizmo_config` because `linestrip_2d` has no per-call width
/// in Bevy 0.19.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct LandBorderGizmoConfigGroup;

/// Spawn one [`LandGraphic`] per land (for the polygon draw). The per-frame
/// [`update`] system walks them and draws the polygon. The name + yield
/// `Text2d` label is spawned by `holding_icon::startup`.
pub fn startup(
    mut commands: Commands,
    // populate has already run by the time Startup schedules, so the land
    // entities exist and this query resolves.
    lands: Query<Entity, With<Land>>,
) {
    for land_e in &lands {
        commands.spawn((LandGraphic, UIWithLand(land_e)));
    }
}

/// Per-frame land outline + fill (brown normally, yellow when selected;
/// brown fill, green-tinted when player-owned). The yield label lives in
/// `holding_icon::update`.
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
}

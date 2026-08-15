//! Visual marker for a land: polygon outline + scanline fill.
//!
//! Outline brown normally, yellow when selected; fill brown normally, green-tinted
//! when the player owns the land. The name + yield label lives in `holding_icon`
//! (both anchor to the same land-holding point).

use super::common::{fill, UIWithLand};
use crate::app::Game;
use crate::ecs::land::{Land, LandBorders};
use crate::ecs::{CharacterLeads, KingdomHold, Registry, StringId};
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use std::collections::HashSet;

/// Marker on the entity that drives the per-land outline + fill draw.
#[derive(Component, Debug, Clone, Copy)]
pub struct LandGraphic;

/// Gizmo config group dedicated to the per-land polygon outline (1.0px stroke).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct LandBorderGizmoConfigGroup;

/// Spawn one `LandGraphic` per land.
pub fn startup(mut commands: Commands, lands: Query<Entity, With<Land>>) {
    for land_e in &lands {
        commands.spawn((LandGraphic, UIWithLand(land_e)));
    }
}

/// Per-frame land outline + fill. Sorts the selected land to the back so it draws over its neighbours.
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
    let own: HashSet<String> = game
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|pe| character_leads.get(pe).ok())
        .map(|character_leads| {
            let mut out = HashSet::new();
            for kingdom_e in character_leads.kingdoms() {
                let Ok(kingdom_hold) = kingdom_holds.get(*kingdom_e) else { continue };
                if let Ok(string_id) = lands_id.get(kingdom_hold.0) {
                    out.insert(string_id.0.clone());
                }
            }
            out
        })
        .unwrap_or_default();

    let mut ordered: Vec<Entity> = icons.iter().map(|ui| ui.0).collect();
    ordered.sort_by_key(|&ui_with_land| {
        lands_id.get(ui_with_land).map(|sid| Some(sid.0.as_str()) == sel).unwrap_or(false)
    });

    for ui_with_land in ordered {
        let Ok(borders) = lands.get(ui_with_land) else { continue };
        let Ok(string_id) = lands_id.get(ui_with_land) else { continue };
        let land_id = string_id.0.as_str();
        let is_sel = Some(land_id) == sel;
        let is_own = own.contains(land_id);

        let outline = if is_sel { css::YELLOW } else { Srgba::rgb(0.36, 0.22, 0.12) };
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

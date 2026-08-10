//! The ARMIES panel in the right-hand column: one row per army standing on
//! the selected land.
//!
//! - **Idle** armies render as `"<ArmyName> (<levy>)"`, e.g. `"Lannister Army (90)"`.
//! - **Marching** armies render as `"<ArmyName> (<levy>) marching to <target> in <N> days"`,
//!   e.g. `"Lannister Army (90) marching to Riverrun in 11 days"`, where
//!   `<target>` is the target land's `LandName` and `<N>` is the days
//!   remaining until the marching's `MarchingArrivedDate`. A marching is one
//!   road, so `<target>` is the end of the road the army is currently on —
//!   the next land it reaches, not necessarily the end of a multi-road route.
//!
//! Mirrors the layout of `ui::courts` (a title + a single multi-line `Text`
//! block) since the population is small — typically 0–3 armies on a land.
//! `update` walks the selected land's `LandHasArmies` (the auto-maintained
//! reverse of `ArmyOnLand`) and renders one line per army; falls back to
//! `(none)` when there's no selection or no army sits on the selected land.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::army::{ArmyLevy, ArmyMarching, ArmyName, ArmyStatus};
use crate::ecs::land::LandHasArmies;
use crate::ecs::marching::{MarchingArrivedDate, MarchingToLand};
use crate::ecs::{LandName, Registry};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Column container for the armies list. Rebuilt by [`update`] each frame.
#[derive(Component)]
pub struct LegendArmies;

/// The ARMIES panel: title + column container of army rows. Spawned as a
/// sibling panel between `buildings` and `actions` in the right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            // Size to content; grow only if a child pushes it (flex item
            // default: `flex_grow: 0` + `min_height: Auto`).
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("ARMIES"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((LegendArmies, Text::new(""), TextFont::from_font_size(FONT)));
    });
}

/// Render one row per army on the selected land. Idle armies show name +
/// levy; marching armies show name + levy + "marching to <target> in <N>
/// days". Clears to `(none)` when the selection doesn't resolve to a land
/// or the land has no armies on it.
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    calendar: Res<Calendar>,
    date: Res<Date>,
    land_has_armies_q: Query<&LandHasArmies>,
    armies: Query<(&ArmyName, &ArmyLevy, &ArmyStatus)>,
    army_marching: Query<&ArmyMarching>,
    marching_to_land: Query<&MarchingToLand>,
    marching_arrived_date: Query<&MarchingArrivedDate>,
    land_names: Query<&LandName>,
    mut text: Single<&mut Text, With<LegendArmies>>,
) {
    let Some(land_e) = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))
    else {
        text.0 = "(none)".into();
        return;
    };
    let Ok(land_has_armies) = land_has_armies_q.get(land_e) else {
        text.0 = "(none)".into();
        return;
    };

    let today_ord = date.ordinal(&calendar);

    // Walk `LandHasArmies` (the land's auto-maintained list) and render one
    // line per army. Missing `ArmyName`/`ArmyLevy`/`ArmyStatus` on an entry
    // skips it — every army spawned by `RaiseArmy` carries all three, but a
    // torn edge case shouldn't crash the panel. For marching armies, a
    // missing `ArmyMarching`/`MarchingToLand`/`MarchingArrivedDate` chain
    // likewise skips the army rather than rendering a half-formed line.
    let mut lines = land_has_armies.iter().filter_map(|army_e| {
        let (name, levy, status) = armies.get(army_e).ok()?;
        let base = format!("{} ({})", name.0, levy.0);
        if *status != ArmyStatus::Marching {
            return Some(base);
        }
        let marching_e = army_marching.get(army_e).ok()?.0;
        let target_e = marching_to_land.get(marching_e).ok()?.0;
        let arrived = marching_arrived_date.get(marching_e).ok()?.0?;
        let target_name = land_names.get(target_e).ok()?.0.clone();
        let days = arrived.ordinal(&calendar) - today_ord;
        Some(format!("{base} marching to {target_name} in {days} days"))
    });

    text.0 = lines
        .next()
        .map(|first| {
            std::iter::once(first)
                .chain(lines)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "(none)".into());
}

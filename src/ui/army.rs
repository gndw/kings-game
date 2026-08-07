//! The ARMIES panel in the right-hand column: armies of the kingdom holding
//! the selected land, one `<ArmyName> (<levy>)` row per army.
//!
//! Mirrors the layout of `ui::courts` (a title + a single multi-line `Text`
//! block) since the population is small — typically 0–3 armies per kingdom,
//! one per raise. `update` resolves the selected land's kingdom, walks its
//! `KingdomHasArmies`, and renders one line per army; falls back to `(none)`
//! when there's nothing to show, matching `ui::courts`.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::army::{ArmyLevy, ArmyName};
use crate::ecs::{KingdomHasArmies, LandHeldBy, Registry};
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

/// Render one `<ArmyName> (<levy>)` row per army under the selected land's
/// kingdom; clears to `(none)` when the selection doesn't resolve to a
/// kingdom-held land or the kingdom has no armies.
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    land_held_by: Query<&LandHeldBy>,
    kingdom_has_armies: Query<&KingdomHasArmies>,
    armies: Query<(&ArmyName, &ArmyLevy)>,
    mut text: Single<&mut Text, With<LegendArmies>>,
) {
    // The selected land's kingdom, via the same `LandHeldBy` lookup the
    // other kingdom-following panels (courts, information) use.
    let kingdom = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|land| land_held_by.get(land).ok())
        .map(LandHeldBy::kingdom);

    let Some(kingdom_e) = kingdom else {
        text.0 = "(none)".into();
        return;
    };
    let Ok(kingdom_has_armies) = kingdom_has_armies.get(kingdom_e) else {
        text.0 = "(none)".into();
        return;
    };

    // Walk `KingdomHasArmies` (the kingdom's auto-maintained list) and
    // render one line per army. Missing `ArmyName`/`ArmyLevy` on an entry
    // skips it — every army spawned by `RaiseArmy` carries both, but a torn
    // edge case shouldn't crash the panel.
    let mut lines = kingdom_has_armies
        .iter()
        .filter_map(|army_e| {
            let (name, levy) = armies.get(army_e).ok()?;
            Some(format!("{} ({})", name.0, levy.0))
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
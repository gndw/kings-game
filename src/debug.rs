//! Debug aids. Disabled in release builds via a build flag would be the long
//! answer; today this module is just a single key-driven dump for diagnostics.

use crate::ecs::{CharacterGold, CharacterGoldYield, CharacterLevy, CharacterName, Registry};
use bevy::prelude::*;

/// Debug: on **I** press, list every character's name, treasury (`gold`),
/// monthly `gold_yield`, and `levy`. Helpful for verifying that the
/// `On<Insert, OnLand>` / `On<Remove, OnLand>` observers in
/// [`crate::updates::yields`] recompute the realm's yield the moment a
/// building is constructed or torn down — the resource bar shows only the
/// player's slice.
pub fn dump_characters(
    keys: Res<ButtonInput<KeyCode>>,
    registry: Res<Registry>,
    chars: Query<(
        &CharacterName,
        &CharacterGold,
        &CharacterGoldYield,
        &CharacterLevy,
    )>,
) {
    if !keys.just_pressed(KeyCode::KeyI) {
        return;
    }
    eprintln!("[debug] character dump:");
    let mut rows: Vec<_> = registry
        .by_id
        .iter()
        .map(|(id, e)| (id, *e))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, entity) in rows {
        let Ok((name, gold, gy, levy)) = chars.get(entity) else {
            continue;
        };
        eprintln!(
            "[debug]   {id:>20}  {}: gold={}  yield={:+}/mo  levy={}",
            name.0,
            gold.0,
            gy.0,
            levy.0
        );
    }
}

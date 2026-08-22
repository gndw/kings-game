//! Debug aids. Disabled in release builds via a build flag would be the long
//! answer; today this module is just a single key-driven dump for diagnostics.

use crate::ecs::{
    Kingdom, KingdomGold, KingdomGoldYield, KingdomLevy, KingdomName, Registry,
};
use bevy::prelude::*;

/// Startup marker: prints a single terminal line once every other startup
/// system has run. The line's appearance is the signal that `cargo run`
/// reached steady state — Bevy, plugins, ECS, all `Startup` systems —
/// without panicking on init. Agents checking compileability watch for
/// this exact string in stderr and stop the process afterwards.
pub fn startup_log_loaded() {
    eprintln!("kings-game: loaded properly");
}

/// Debug: on **I** press, list every kingdom's name, treasury (`gold`),
/// monthly `gold_yield`, and `levy`. Helpful for verifying that the
/// `On<Insert, BuildingOnLand>` / `On<Remove, BuildingOnLand>` observers in
/// [`crate::game::yielding`] recompute the realm's yield the moment a
/// building is constructed or torn down — the resource bar shows only the
/// player's slice.
pub fn dump_kingdoms(
    keys: Res<ButtonInput<KeyCode>>,
    registry: Res<Registry>,
    kingdoms: Query<(
        &KingdomName,
        &KingdomGold,
        &KingdomGoldYield,
        &KingdomLevy,
    ), With<Kingdom>>,
) {
    if !keys.just_pressed(KeyCode::KeyI) {
        return;
    }
    eprintln!("[debug] kingdom dump:");
    let mut rows: Vec<_> = registry
        .by_id
        .iter()
        .filter(|(id, _)| id.starts_with("kingdom-"))
        .map(|(id, e)| (id, *e))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, entity) in rows {
        let Ok((name, gold, gy, levy)) = kingdoms.get(entity) else {
            continue;
        };
        eprintln!(
            "[debug]   {id:>30}  {}: gold={}  yield={:+}/mo  levy={}",
            name.0, gold.0, gy.0, levy.0
        );
    }
}

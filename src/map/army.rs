//! On-map army indicators: for each `Army` entity, ensure a sibling
//! `ArmyIcon` entity exists at the army's land holding point. The drawing
//! (gizmos + name label) is handled by
//! [`crate::map::components::army_icon::update`], which iterates entities
//! carrying the `ArmyIcon` marker.
//!
//! This module owns the army → ArmyIcon lifecycle: lazy-spawn on first
//! sight, position-sync each frame (so a marching army drags its icon
//! along), reap on army despawn. `army_icon::update` owns its own
//! label-entity lifecycle (text + bg sprite) and reaps them when the icon
//! is despawned — the cross-system cascade goes
//! `RemovedComponents<Army>` → despawn `ArmyIcon` →
//! `RemovedComponents<ArmyIcon>` → despawn text + bg.
//!
//! Runs in `PostUpdate` alongside `army_icon::update` (which runs after
//! this one in the schedule so the icon transform is settled by the time
//! the gizmos draw).

use super::components::army_icon::{ArmyIcon, ArmyIconLabel};
use crate::ecs::army::{Army, ArmyLevy, ArmyName, ArmyOnLand};
use crate::ecs::LandHolding;
use bevy::prelude::*;
use std::collections::HashMap;

/// Per-frame: ensure one `ArmyIcon` entity exists per army, parked at the
/// army's land holding point. Existing icons get their `Transform` kept
/// in sync with the army's current land so marching armies drag the icon
/// along, and the label is refreshed to the latest `Name (Levy)`. New
/// icons are spawned with the same initial label.
pub fn update(
    mut commands: Commands,
    armies: Query<(Entity, &ArmyOnLand, &ArmyName, &ArmyLevy), With<Army>>,
    lands: Query<&LandHolding>,
    mut removed: RemovedComponents<Army>,
    // ponytail: cache army → icon entity in a Local so we don't respawn
    // every frame; the population is bounded by the army count (small).
    mut army_icons: Local<HashMap<Entity, Entity>>,
    mut icon_transforms: Query<&mut Transform, With<ArmyIcon>>,
    mut icon_labels: Query<&mut ArmyIconLabel, With<ArmyIcon>>,
) {
    // Reap icons for armies that have been despawned (DismissArmy,
    // marching tick, …). `army_icon::update`'s own reaper picks up the
    // resulting `RemovedComponents<ArmyIcon>` and despawns the text + bg
    // siblings.
    for r in removed.read() {
        if let Some(icon_e) = army_icons.remove(&r) {
            commands.entity(icon_e).despawn();
        }
    }

    for (army_e, army_on_land, army_name, army_levy) in &armies {
        // Resolve land holding point. Skip silently if the land has been
        // dropped — despawn should have removed the army too, but a torn
        // edge case shouldn't crash the frame.
        let Ok(holding) = lands.get(army_on_land.0) else {
            continue;
        };
        let pos = Vec3::new(holding.0.0 as f32, holding.0.1 as f32, 0.0);
        let label = format!("{} ({})", army_name.0, army_levy.0);

        // Existing icon: keep transform + label in sync. New icon: spawn
        // at the land's holding point with the label baked in.
        if let Some(&icon_e) = army_icons.get(&army_e) {
            if let Ok(mut t) = icon_transforms.get_mut(icon_e) {
                t.translation = pos;
            }
            if let Ok(mut l) = icon_labels.get_mut(icon_e) {
                l.0 = label;
            }
        } else {
            let icon_e = commands
                .spawn((
                    ArmyIcon,
                    ArmyIconLabel(label),
                    Transform::from_translation(pos),
                ))
                .id();
            army_icons.insert(army_e, icon_e);
        }
    }
}

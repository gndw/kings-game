//! Army indicators on the map: a small military-style marker at the land's
//! holding point plus the army's `ArmyName` next to it, per army.
//!
//! Spawns a `Text2d` label the first frame it sees an army without one and
//! tracks the mapping in a `Local<HashMap>`. When an army is despawned
//! (`RemovedComponents<Army>`) its label is despawned too. Existing labels
//! have their text refreshed every frame the army could be renamed or its
//! levy change.
//!
//! Runs in `PostUpdate` alongside `crate::ui::map::update_draw` — the camera
//! is already framed for this frame, so the indicators land on top of the
//! map layer.

use super::FONT_SIZE;
use crate::ecs::army::{Army, ArmyLevy, ArmyName};
use crate::ecs::LandHolding;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashMap;

/// `Text2d` entity spawned as the on-map label for one army. The `Entity`
/// inside is the army's entity, so the text is refreshed by looking up the
/// army's name and levy in [`update`].
#[derive(Component)]
pub struct ArmyLabel(pub Entity);

/// World-space size of the marker square drawn for each army. Fixed like the
/// holding circle so it doesn't change with zoom.
const MARKER: f32 = 8.0;
/// Vertical gap between stacked army markers on the same land.
const ROW: f32 = 16.0;
/// Gap between the marker's top edge and the label's bottom edge. Keeps the
/// text from kissing the square while still reading as one group.
const LABEL_GAP: f32 = 4.0;

/// Per-frame: draw markers + labels for every army. Spawn a label for any
/// army that doesn't have one; refresh the text on existing ones; despawn
/// the labels of armies that have been removed.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    mut gizmos: Gizmos,
    armies: Query<(Entity, &crate::ecs::army::ArmyOnLand, &ArmyName, &ArmyLevy), With<Army>>,
    lands: Query<&LandHolding>,
    mut removed: RemovedComponents<Army>,
    // ponytail: cache army → label entity in a Local so we don't respawn every
    // frame; hashmap lookup is O(1) and the population is bounded by armies
    // (small).
    mut labels: Local<HashMap<Entity, Entity>>,
    mut label_q: Query<&mut Text2d, With<ArmyLabel>>,
) {
    // Reap labels for armies that have been despawned.
    for r in removed.read() {
        if let Some(label_e) = labels.remove(&r) {
            commands.entity(label_e).despawn();
        }
    }

    // Per-land stack index: lets multiple armies on the same land stack their
    // markers vertically instead of overlapping on the holding point.
    let mut stack: HashMap<Entity, usize> = HashMap::new();

    for (army_e, army_on_land, army_name, army_levy) in &armies {
        let land_e = army_on_land.0;
        let idx = *stack.get(&land_e).unwrap_or(&0);
        stack.insert(land_e, idx + 1);

        // Resolve land position. Skip silently if the land has been dropped —
        // despawn should have removed the army, but a torn edge case shouldn't
        // crash the frame.
        let Ok(holding) = lands.get(land_e) else {
            continue;
        };

        // Marker: a small red square above the holding, stacked per-army.
        let cx = holding.0.0 as f32;
        let cy = holding.0.1 as f32 + 6.0 + idx as f32 * ROW;
        let marker_y = cy + 14.0;
        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(cx, marker_y)),
            Vec2::splat(MARKER),
            css::RED,
        );

        // Label: ensure a Text2d exists for this army, then set its content.
        // `Anchor::BOTTOM_LEFT` puts the position at the text's bottom-left,
        // so the label grows upward from `label_y` — which sits a small gap
        // above the marker's top edge. Created once and despawned via the
        // removed-army reaper above.
        let label_e = if let Some(&e) = labels.get(&army_e) {
            e
        } else {
            let label_y = marker_y + MARKER / 2.0 + LABEL_GAP;
            let e = commands
                .spawn((
                    Text2d::new(""),
                    TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::BOLD),
                    TextColor(Color::Srgba(css::RED)),
                    TextLayout::new(Justify::Left, LineBreak::WordBoundary),
                    Anchor::BOTTOM_LEFT,
                    ArmyLabel(army_e),
                    Transform::from_xyz(cx + 10.0, label_y, 1.0),
                ))
                .id();
            labels.insert(army_e, e);
            e
        };
        if let Ok(mut t) = label_q.get_mut(label_e) {
            t.0 = format!("{} ({})", army_name.0, army_levy.0);
        }
    }
}
//! Visual marker for a holding (castle) on the map: a white-line castle
//! silhouette with three towers (centre taller than sides), crenellations
//! on every tower top, connecting walls at the side-tower height, and a
//! central gate. An optional centred bold-white name label sits just
//! below the gate, on a black `Sprite` background that auto-sizes to the
//! rendered text.
//!
//! Visual-only — placement and lifecycle are the caller's job.

use super::super::FONT_SIZE;
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;
use std::collections::HashMap;

/// Marker on an entity whose world translation is the anchor point for
/// the holding-icon visual (the ground at the castle's base).
#[derive(Component, Debug, Clone, Copy)]
pub struct HoldingIcon;

/// The text rendered below the castle, centred under the gate. Optional
/// — an icon without this component draws the gizmo visual only.
#[derive(Component, Debug, Clone)]
pub struct HoldingIconLabel(pub String);

/// Marker on the `Text2d` entity spawned for an icon's label. Lets
/// [`update`] refresh the text and reaps it on icon despawn.
#[derive(Component)]
pub struct HoldingIconText;

/// Back-reference from a label's black background `Sprite` to its `Text2d`,
/// used by [`update`] to size the sprite to the text.
#[derive(Component)]
pub struct HoldingIconLabelBg(pub Entity);

// Castle proportions, world units. Sized to sit next to the army icon
// (50-unit pole) at comparable visual weight without dwarfing it.
const TOWER_W: f32 = 8.0;
const TOWER_H: f32 = 22.0;
const SIDE_TOWER_H: f32 = 14.0;
const TOWER_SPACING: f32 = 13.0;
const GATE_W: f32 = 4.0;
const GATE_H: f32 = 6.0;
const CRENEL_W: f32 = 2.0;
const CRENEL_GAP: f32 = 2.0;
const CRENEL_DEPTH: f32 = 2.0;

/// Vertical gap between the castle's base (`at.y`) and the label's top
/// edge. The label sits below the gate.
const LABEL_GAP: f32 = 6.0;
/// Z-order: the black background sits just behind the white text so the
/// text renders on top of it.
const LABEL_BG_Z: f32 = 0.9;

/// Draw the castle silhouette in `color` lines at world point `at`. `at`
/// is the bottom-centre of the castle (ground level).
///
/// Drawn back-to-front by z-order of gizmo draws in the frame: walls
/// first, then towers (which overlap the wall tops), then crenellations
/// and the gate last. All in the default gizmo group; the relative order
/// within a single `draw` call is what matters visually.
pub fn draw(gizmos: &mut Gizmos, at: Vec2, color: Srgba) {
    let tower_xs = [at.x - TOWER_SPACING, at.x, at.x + TOWER_SPACING];
    let tower_heights = [SIDE_TOWER_H, TOWER_H, SIDE_TOWER_H];

    // Wall sections between adjacent towers, at side-tower height. The
    // walls visually connect the three towers into a single silhouette.
    for i in 0..2 {
        let prev_right = tower_xs[i] + TOWER_W / 2.0;
        let next_left = tower_xs[i + 1] - TOWER_W / 2.0;
        let wall_w = next_left - prev_right;
        let wall_center_x = (prev_right + next_left) / 2.0;
        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(
                wall_center_x,
                at.y + SIDE_TOWER_H / 2.0,
            )),
            Vec2::new(wall_w, SIDE_TOWER_H),
            color,
        );
    }

    // Three towers: side towers shorter, centre taller. The central tower
    // extends above the wall line for the iconic "taller keep" silhouette.
    for (i, &tx) in tower_xs.iter().enumerate() {
        let h = tower_heights[i];
        let top = at.y + h;
        let left = tx - TOWER_W / 2.0;
        let right = tx + TOWER_W / 2.0;

        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(tx, at.y + h / 2.0)),
            Vec2::new(TOWER_W, h),
            color,
        );

        // Crenellations on top of every tower.
        crenellations(gizmos, left, right, top, color);
    }

    // Gate: small rectangle at the centre of the central tower's base.
    // The outline overlaps the tower's bottom edge — visually it reads as
    // an inset doorway at this scale.
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, at.y + GATE_H / 2.0)),
        Vec2::new(GATE_W, GATE_H),
        color,
    );
}

/// Draw crenellations (battlements) along the top of a tower as a single
/// zig-zag polyline: alternating short upward segments (teeth) and flat
/// segments (gaps). Towers narrow enough to fit no teeth at all draw no
/// crenellations (the constant `pitch` would exceed `tower_w`).
fn crenellations(gizmos: &mut Gizmos, left: f32, right: f32, top: f32, color: Srgba) {
    let pitch = CRENEL_W + CRENEL_GAP;
    let tower_w = right - left;
    let n_teeth = (tower_w / pitch).floor() as i32;
    if n_teeth == 0 {
        return;
    }

    // Centre the tooth pattern in the tower's width.
    let total_w = n_teeth as f32 * CRENEL_W + (n_teeth as f32 - 1.0) * CRENEL_GAP;
    let start = left + (tower_w - total_w) / 2.0;

    let mut path = vec![Vec2::new(left, top)];
    let mut x = start;
    for _ in 0..n_teeth {
        // Up to tooth top, across tooth, down to wall top, across gap.
        path.push(Vec2::new(x, top + CRENEL_DEPTH));
        path.push(Vec2::new(x + CRENEL_W, top + CRENEL_DEPTH));
        path.push(Vec2::new(x + CRENEL_W, top));
        path.push(Vec2::new(x + pitch, top));
        x += pitch;
    }
    path.push(Vec2::new(right, top));

    gizmos.linestrip_2d(path.iter().copied(), color);
}

/// Spawn the `Text2d` and its matching black background `Sprite` at world
/// point `anchor`. Returns `(text_e, bg_e)`.
///
/// The background sprite starts at `1×1`; [`update`] resizes it to match
/// the text's `TextLayoutInfo` from the second frame onward. The first
/// frame shows a 1×1 black square; on the next frame it expands to fit.
fn spawn_label(commands: &mut Commands, text: &str, anchor: Vec2) -> (Entity, Entity) {
    let text_e = commands
        .spawn((
            Text2d::new(text.to_string()),
            TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
            TextColor(Color::Srgba(css::WHITE)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            Anchor::TOP_CENTER,
            HoldingIconText,
            Transform::from_xyz(anchor.x, anchor.y, 1.0),
        ))
        .id();
    let bg_e = commands
        .spawn((
            Sprite {
                color: Color::BLACK,
                custom_size: Some(Vec2::new(1.0, 1.0)),
                ..default()
            },
            Transform::from_xyz(anchor.x, anchor.y, LABEL_BG_Z),
            HoldingIconLabelBg(text_e),
        ))
        .id();
    (text_e, bg_e)
}

/// Draw the castle at every entity carrying [`HoldingIcon`]; for entities
/// also carrying [`HoldingIconLabel`], spawn (or reuse) a centred
/// bold-white name label below the gate on a black background sprite
/// sized to the rendered text.
///
/// Labels are spawned lazily the first frame an icon-with-label is seen
/// and cached in a `Local<HashMap>` so the entities aren't respawned every
/// frame. The text is refreshed each frame so a future rename command
/// updates the map live. When a `HoldingIcon` is despawned its text + bg
/// sprite are reaped via `RemovedComponents<HoldingIcon>`.
///
/// The `Without<HoldingIconLabelBg>` / `Without<HoldingIcon>` filters on
/// the icons/bg queries make them provably disjoint for Bevy's access
/// check — both touch `Transform` on different filter sets, but the
/// filters make the disjointness explicit so the check passes without a
/// `ParamSet`.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    mut gizmos: Gizmos,
    icons: Query<
        (Entity, &Transform, Option<&HoldingIconLabel>),
        (With<HoldingIcon>, Without<HoldingIconLabelBg>),
    >,
    mut removed: RemovedComponents<HoldingIcon>,
    // ponytail: cache icon → (text, bg) entity pair in a Local so we don't
    // respawn every frame; hashmap lookup is O(1) and the population is
    // bounded by holdings (small).
    mut labels: Local<HashMap<Entity, (Entity, Entity)>>,
    mut text_q: Query<&mut Text2d, With<HoldingIconText>>,
    mut bg_q: Query<
        (&mut Sprite, &mut Transform),
        (With<HoldingIconLabelBg>, Without<HoldingIcon>),
    >,
    layout_q: Query<&TextLayoutInfo>,
) {
    // Reap labels for icons that have been despawned. Both the text and the
    // background sprite are despawned explicitly so the next frame's
    // `bg_q.get_mut` can't match a stale entity.
    for r in removed.read() {
        if let Some((text_e, bg_e)) = labels.remove(&r) {
            for e in [text_e, bg_e] {
                if let Ok(mut ec) = commands.get_entity(e) {
                    ec.despawn();
                }
            }
        }
    }

    for (icon_e, t, label) in &icons {
        // Castle visual at the entity's translation.
        draw(&mut gizmos, t.translation.truncate(), css::WHITE);

        // Label: spawn on first sight, then refresh text + resize background
        // every frame so a future rename command updates the map live.
        let Some(label) = label else { continue };
        let anchor = Vec2::new(t.translation.x, t.translation.y - LABEL_GAP);
        if let Some(&(text_e, bg_e)) = labels.get(&icon_e) {
            // Refresh text content.
            if let Ok(mut txt) = text_q.get_mut(text_e) {
                txt.0 = label.0.clone();
            }
            // Resize the bg sprite to the text's measured size. Layout info
            // is populated by Bevy after the first frame the text exists, so
            // the if-let skips the spawn-frame `1×1` placeholder rather than
            // panicking on a missing layout.
            if let Ok(layout) = layout_q.get(text_e)
                && let Ok((mut sprite, mut bg_t)) = bg_q.get_mut(bg_e)
            {
                sprite.custom_size = Some(layout.size);
                // Text is `TOP_CENTER`-anchored, so its bbox runs from
                // `anchor` (top) to `anchor + (0, -size.y)` (bottom). Centre
                // the sprite on the bbox so the bg extends symmetrically
                // around the text.
                bg_t.translation = Vec3::new(anchor.x, anchor.y - layout.size.y / 2.0, LABEL_BG_Z);
            }
        } else {
            let pair = spawn_label(&mut commands, &label.0, anchor);
            labels.insert(icon_e, pair);
        }
    }
}

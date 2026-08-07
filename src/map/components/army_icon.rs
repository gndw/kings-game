//! Visual marker for an army on the map: three yellow stick-figure soldiers
//! standing at the base, a white flag pole rising from the middle behind
//! them, and a waving red triangular flag at the top. An optional centred
//! bold-white name label sits just above the flag, on a black `Sprite`
//! background that auto-sizes to the rendered text.
//!
//! Visual-only — placement and lifecycle are the caller's job. The on-map
//! per-army indicator at each land's holding point lives in
//! [`crate::map::army`]; this module is the reusable visual that rides on
//! top of every `Army` entity.

use super::super::FONT_SIZE;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;
use std::collections::HashMap;

/// Marker on an entity whose world translation is the anchor point for the
/// army-icon visual. [`update`] draws one icon per entity every frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyIcon;

/// The text rendered below the icon, centred under the soldiers. Optional
/// — an icon without this component draws the gizmo visual only.
#[derive(Component, Debug, Clone)]
pub struct ArmyIconLabel(pub String);

/// Marker on the `Text2d` entity spawned for an icon's label. Lets
/// [`update`] refresh the text and reaps it on icon despawn.
#[derive(Component)]
pub struct ArmyIconText;

/// Back-reference from a label's black background `Sprite` to its `Text2d`,
/// used by [`update`] to size the sprite to the text.
#[derive(Component)]
pub struct ArmyIconLabelBg(pub Entity);

// Pole / flag proportions, world units. Sized so the icon sits comfortably
// over a holding circle without dwarfing it.
const POLE: f32 = 50.0;
const FLAG_W: f32 = 36.0;
const FLAG_H: f32 = 24.0;
/// Even number of rows so the widest scanline lands at the flag's mid-height.
const ROWS: usize = 10;
const SEGS: usize = 20;
const PERSON_DX: f32 = 10.0;
const HEAD_R: f32 = 3.5;
const BODY: f32 = 8.0;
const LEG: f32 = 3.0;

/// Vertical gap between the pole's top (`at.y + POLE`) and the label's
/// top edge. The label sits above the flag.
const LABEL_GAP: f32 = 30.0;
/// Z-order: the black background sits just behind the white text so the text
/// renders on top of it.
const LABEL_BG_Z: f32 = 0.9;

/// Wave displacement at the cloth point `(phase, t)` where `t ∈ [0, 1]` is
/// the cloth's position from pole (`0`) to fly (`1`). Slack grows with `t`,
/// so the pole edge stays put and the tip whips. Mirrors the selected-land
/// pennant in [`crate::ui::flag`].
fn wave(phase: f32, t: f32) -> f32 {
    (phase * 4.0 - t * 4.0).sin() * 2.0 * t
}

/// Draw the army-icon visual at world point `at`. `phase` is seconds since
/// startup, used to animate the flag wave.
///
/// Drawn back-to-front so the layering reads correctly: pole (back) → flag
/// → soldiers (front). The pole is drawn first so the soldiers overlap it
/// (the visual reads as "soldiers standing in front of the flag"); the flag
/// sits above the pole's top so its z-order vs the soldiers is irrelevant.
pub fn draw(gizmos: &mut Gizmos, at: Vec2, phase: f32) {
    let pole_top = at + Vec2::new(0.0, POLE);
    gizmos.line_2d(at, pole_top, css::WHITE);

    // Triangular pennant: full width at the middle of the flag's height,
    // tapering to a point at the top and bottom. Same shape as the
    // selected-land flag in `crate::ui::flag` so the two visuals match.
    for i in 0..=ROWS {
        let v = i as f32 / ROWS as f32;
        let row_y = pole_top.y - v * FLAG_H;
        let len = FLAG_W * (1.0 - 2.0 * (v - 0.5).abs());
        for j in 0..SEGS {
            let (t0, t1) = (j as f32 / SEGS as f32, (j + 1) as f32 / SEGS as f32);
            gizmos.line_2d(
                Vec2::new(pole_top.x + t0 * len, row_y + wave(phase, t0)),
                Vec2::new(pole_top.x + t1 * len, row_y + wave(phase, t1)),
                css::RED,
            );
        }
    }

    // Three yellow stick figures side by side. Head circle on top of the
    // body line; arms as a short horizontal line at body mid-height.
    for sign in [-1.0_f32, 0.0, 1.0] {
        let cx = at.x + sign * PERSON_DX;
        let head_y = at.y + BODY;
        gizmos.line_2d(Vec2::new(cx, at.y), Vec2::new(cx, head_y), css::YELLOW);
        gizmos.circle_2d(
            Isometry2d::from_translation(Vec2::new(cx, head_y + HEAD_R)),
            HEAD_R,
            css::YELLOW,
        );
        let arms_y = at.y + BODY * 0.6;
        gizmos.line_2d(
            Vec2::new(cx - LEG, arms_y),
            Vec2::new(cx + LEG, arms_y),
            css::YELLOW,
        );
    }
}

/// Spawn the `Text2d` and its matching black background `Sprite` at world
/// point `anchor`. Returns `(text_e, bg_e)`.
///
/// The background sprite starts at `1×1`; [`update`] resizes it to match the
/// text's `TextLayoutInfo` from the second frame onward. The first frame
/// shows a 1×1 black square; on the next frame it expands to fit.
fn spawn_label(commands: &mut Commands, text: &str, anchor: Vec2) -> (Entity, Entity) {
    let text_e = commands
        .spawn((
            Text2d::new(text.to_string()),
            TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
            TextColor(Color::Srgba(css::WHITE)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            Anchor::TOP_CENTER,
            ArmyIconText,
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
            ArmyIconLabelBg(text_e),
        ))
        .id();
    (text_e, bg_e)
}

/// Draw the army-icon visual at every entity carrying [`ArmyIcon`]; for
/// entities also carrying [`ArmyIconLabel`], spawn (or reuse) a centred
/// bold-white name label below the soldiers on a black background sprite
/// sized to the rendered text.
///
/// Labels are spawned lazily the first frame an icon-with-label is seen and
/// cached in a `Local<HashMap>` so the entities aren't respawned every
/// frame. The text is refreshed each frame so a future rename command
/// updates the map live. When an `ArmyIcon` is despawned its text + bg
/// sprite are reaped via `RemovedComponents<ArmyIcon>`.
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    mut gizmos: Gizmos,
    // `Without<ArmyIconLabelBg>` on the icons query and `Without<ArmyIcon>`
    // on the bg query make the two provably disjoint for Bevy's access
    // check: `icons` reads `&Transform` and `bg_q` writes `&mut Transform`,
    // and the only way Bevy will accept that without a `ParamSet` is if it
    // can prove the two queries never match the same entity.
    icons: Query<(Entity, &Transform, Option<&ArmyIconLabel>), (With<ArmyIcon>, Without<ArmyIconLabelBg>)>,
    mut removed: RemovedComponents<ArmyIcon>,
    time: Res<Time>,
    // ponytail: cache icon → (text, bg) entity pair in a Local so we don't
    // respawn every frame; hashmap lookup is O(1) and the population is
    // bounded by armies (small).
    mut labels: Local<HashMap<Entity, (Entity, Entity)>>,
    mut text_q: Query<&mut Text2d, With<ArmyIconText>>,
    mut bg_q: Query<(&mut Sprite, &mut Transform), (With<ArmyIconLabelBg>, Without<ArmyIcon>)>,
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
        // Icon visual at the entity's translation.
        draw(&mut gizmos, t.translation.truncate(), time.elapsed_secs());

        // Label: spawn on first sight, then refresh text + resize background
        // every frame so a future rename command updates the map live.
        let Some(label) = label else { continue };
        // Label sits above the flag, `LABEL_GAP` past the pole's top. Text
        // is `TOP_CENTER`-anchored so it grows downward from here.
        let anchor = Vec2::new(t.translation.x, t.translation.y + POLE + LABEL_GAP);
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

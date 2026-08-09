//! Visual marker for an army on the map: a white-line sword silhouette
//! pointing up — pommel at the ground, then grip, wide crossguard, and a
//! tall blade on top. An optional centred bold-white name label sits just
//! above the blade tip, on a black `Sprite` background that auto-sizes to
//! the rendered text.
//!
//! Lifecycle is event-driven: [`on_army_raised`] spawns the icon + text +
//! bg trio when an army is raised, and [`on_army_dismiss`] despawns them
//! when the army is dismissed. Per-frame work splits across three systems:
//!
//! - [`update`] — position the icon at the army's current land.
//! - [`draw_icons`] — draw the sword gizmo at the icon's position.
//! - [`size_labels`] — keep the text and its black bg sprite positioned +
//!   sized correctly as the army moves.
//!
//! Visual-only — placement and lifecycle are the caller's job. Mirrors
//! [`holding_icon`](super::holding_icon).

use super::super::FONT_SIZE;
use crate::ecs::army::{ArmyLevy, ArmyName, ArmyOnLand};
use crate::ecs::land::LandHolding;
use crate::events::{OnArmyDismiss, OnArmyRaised};
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;

/// Marker on an entity whose world translation is the anchor point for
/// the army-icon visual (the ground at the sword's pommel).
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyIcon;

/// Back-reference from an icon to the army it represents. The per-frame
/// [`update`] system reads `ArmyOnLand` through this and copies the
/// resulting position into the icon's `Transform`.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithArmy(pub Entity);

/// Back-references from the icon entity to its label children. The dismiss
/// observer uses this to clean up the trio in one pass; [`size_labels`]
/// uses it to keep text + bg aligned with the icon.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyIconLabelEntities {
    pub text: Entity,
    pub bg: Entity,
}

/// Marker on the `Text2d` entity spawned for an icon's label.
#[derive(Component)]
pub struct ArmyIconText;

/// Marker on the black-background `Sprite` spawned behind the text.
#[derive(Component)]
pub struct ArmyIconLabelBg;

// Sword proportions, world units. Sized to sit next to the holding-icon
// castle (≈40 units tall) at comparable visual weight.
const BLADE_W: f32 = 4.0;
const BLADE_H: f32 = 30.0;
const CROSSGUARD_W: f32 = 16.0;
const CROSSGUARD_H: f32 = 3.0;
const GRIP_W: f32 = 3.0;
const GRIP_H: f32 = 6.0;
const POMMEL_W: f32 = 5.0;
const POMMEL_H: f32 = 4.0;

/// Total sword height from pommel bottom to blade tip.
const SWORD_H: f32 = POMMEL_H + GRIP_H + CROSSGUARD_H + BLADE_H;

/// Vertical gap between the blade tip (`at.y + SWORD_H`) and the label's
/// bottom edge.
const LABEL_GAP: f32 = 6.0;
/// Z-order: the black background sits just behind the white text so the
/// text renders on top of it.
const LABEL_BG_Z: f32 = 0.9;

/// Draw the sword silhouette in `color` lines at world point `at`. `at` is
/// the bottom-centre of the pommel (ground level).
///
/// Drawn bottom-up: pommel first, then grip, crossguard, blade last. All
/// in the default gizmo group; the relative order within a single `draw`
/// call is what matters visually.
pub fn draw(gizmos: &mut Gizmos, at: Vec2, color: Srgba) {
    // Pommel at the bottom: small wide rectangle.
    let pommel_center_y = at.y + POMMEL_H / 2.0;
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, pommel_center_y)),
        Vec2::new(POMMEL_W, POMMEL_H),
        color,
    );
    let grip_bottom_y = at.y + POMMEL_H;

    // Grip sitting on the pommel: narrow tall rectangle.
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, grip_bottom_y + GRIP_H / 2.0)),
        Vec2::new(GRIP_W, GRIP_H),
        color,
    );
    let crossguard_bottom_y = grip_bottom_y + GRIP_H;

    // Crossguard: wide thin rectangle just above the grip.
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(
            at.x,
            crossguard_bottom_y + CROSSGUARD_H / 2.0,
        )),
        Vec2::new(CROSSGUARD_W, CROSSGUARD_H),
        color,
    );
    let blade_bottom_y = crossguard_bottom_y + CROSSGUARD_H;

    // Blade: long thin rectangle from crossguard top to sword tip.
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, blade_bottom_y + BLADE_H / 2.0)),
        Vec2::new(BLADE_W, BLADE_H),
        color,
    );
}

/// Spawn the `Text2d` and its black background `Sprite` at world point
/// `anchor` (the bottom-centre of the text). Returns `(text_e, bg_e)`.
///
/// The background sprite starts at `1×1`; [`size_labels`] resizes it to
/// match the text's `TextLayoutInfo` from the second frame onward.
fn spawn_label(commands: &mut Commands, text: &str, anchor: Vec2) -> (Entity, Entity) {
    let text_e = commands
        .spawn((
            Text2d::new(text.to_string()),
            TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
            TextColor(Color::Srgba(css::WHITE)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            // `BOTTOM_CENTER`: the text body extends UPWARD from `anchor`,
            // so placing the anchor at `sword_tip_y + LABEL_GAP` puts the
            // label cleanly above the blade tip (text bottom touches the
            // gap line).
            Anchor::BOTTOM_CENTER,
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
            ArmyIconLabelBg,
        ))
        .id();
    (text_e, bg_e)
}

/// Format the on-map label from the army's name + current levy:
/// `"Lannister Army (90)"`. Falls back to `"Army"` when the army has no
/// `ArmyName` (a mod can rename later; the name is read fresh each frame
/// from `update` so renames propagate without restarting the icon).
fn format_label(name: Option<&ArmyName>, levy: &ArmyLevy) -> String {
    let name = name.map(|army_name| army_name.0.as_str()).unwrap_or("Army");
    format!("{name} ({})", levy.0)
}

/// Observer for [`OnArmyRaised`]: spawn the icon, text, and bg at the
/// army's `ArmyOnLand` position. The initial label is formatted with the
/// current levy so the first frame already reads `"Name (N)"` — `update`
/// then keeps it in sync as the levy changes.
pub fn on_army_raised(
    trigger: On<OnArmyRaised>,
    mut commands: Commands,
    armies: Query<
        (&ArmyOnLand, Option<&ArmyName>, &ArmyLevy),
        With<crate::ecs::army::Army>,
    >,
    lands: Query<&LandHolding>,
) {
    let army_e = trigger.event().army;
    let Ok((army_on_land, army_name, army_levy)) = armies.get(army_e) else {
        return;
    };
    let Ok(land_holding) = lands.get(army_on_land.0) else {
        return;
    };

    let pos = Vec2::new(land_holding.0.0 as f32, land_holding.0.1 as f32);
    let label = format_label(army_name, army_levy);

    let text_anchor = Vec2::new(pos.x, pos.y + SWORD_H + LABEL_GAP);
    let (text_e, bg_e) = spawn_label(&mut commands, &label, text_anchor);

    commands.spawn((
        ArmyIcon,
        UIWithArmy(army_e),
        ArmyIconLabelEntities { text: text_e, bg: bg_e },
        Transform::from_xyz(pos.x, pos.y, 0.0),
    ));
}

/// Observer for [`OnArmyDismiss`]: despawn the icon + text + bg trio for
/// the dismissed army. The label entities are reached through the icon's
/// [`ArmyIconLabelEntities`] back-ref.
pub fn on_army_dismiss(
    trigger: On<OnArmyDismiss>,
    mut commands: Commands,
    icons: Query<(Entity, &UIWithArmy, &ArmyIconLabelEntities), With<ArmyIcon>>,
) {
    let army_e = trigger.event().army;
    for (icon_e, ui_with_army, label_ents) in &icons {
        if ui_with_army.0 != army_e {
            continue;
        }
        // Despawn the icon + both label entities together so the next
        // frame's queries can't match a stale sprite/text.
        for e in [icon_e, label_ents.text, label_ents.bg] {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        return;
    }
}

/// Per-frame icon update: positions the icon at the army's current land,
/// draws the sword gizmo, refreshes the text label with the current levy,
/// and fits the bg sprite to the text. One pass per icon.
///
/// The three queries all touch `Transform`; each one carries `Without<...>`
/// for the other two markers so Bevy's B0001 disjointness check can see
/// they're querying three separate entity sets (icon, text, bg).
pub fn update(
    mut icons: Query<
        (&UIWithArmy, &mut Transform, &ArmyIconLabelEntities),
        With<ArmyIcon>,
    >,
    mut text_q: Query<
        (&mut Transform, &mut Text2d, &TextLayoutInfo),
        (
            With<ArmyIconText>,
            Without<ArmyIcon>,
            Without<ArmyIconLabelBg>,
        ),
    >,
    mut bg_q: Query<
        (&mut Sprite, &mut Transform),
        (With<ArmyIconLabelBg>, Without<ArmyIcon>, Without<ArmyIconText>),
    >,
    armies: Query<
        (&ArmyOnLand, Option<&ArmyName>, &ArmyLevy),
        With<crate::ecs::army::Army>,
    >,
    lands: Query<&LandHolding>,
    mut gizmos: Gizmos,
) {
    for (ui_with_army, mut icon_t, label_ents) in &mut icons {
        let Ok((army_on_land, army_name, army_levy)) = armies.get(ui_with_army.0) else {
            continue;
        };
        let Ok(land_holding) = lands.get(army_on_land.0) else {
            continue;
        };

        let pos = Vec2::new(land_holding.0.0 as f32, land_holding.0.1 as f32);

        // Position the icon at the army's current land, then draw the
        // sword gizmo at that point.
        icon_t.translation = pos.extend(icon_t.translation.z);
        draw(&mut gizmos, pos, css::WHITE);

        // Refresh the text content + position it just above the sword tip.
        let Ok((mut text_t, mut text, layout)) = text_q.get_mut(label_ents.text) else {
            continue;
        };
        text.0 = format_label(army_name, army_levy);
        let text_anchor =
            Vec2::new(pos.x, pos.y + SWORD_H + LABEL_GAP);
        text_t.translation = text_anchor.extend(text_t.translation.z);

        // Layout info is populated by Bevy after the first frame the text
        // exists; skip the spawn-frame `1×1` placeholder rather than
        // panicking on a missing layout.
        if layout.size.x <= 0.0 || layout.size.y <= 0.0 {
            continue;
        }

        // Text is `BOTTOM_CENTER`-anchored: bbox runs from `anchor`
        // (bottom) to `anchor + (0, size.y)` (top). Centre the bg sprite
        // on the bbox so it extends symmetrically around the text.
        let Ok((mut sprite, mut bg_t)) = bg_q.get_mut(label_ents.bg) else {
            continue;
        };
        sprite.custom_size = Some(layout.size);
        bg_t.translation =
            Vec3::new(text_anchor.x, text_anchor.y + layout.size.y / 2.0, LABEL_BG_Z);
    }
}

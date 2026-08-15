//! Visual marker for an army on the map: a white-line sword silhouette
//! pointing up, plus an optional name + levy label above it.
//!
//! Lifecycle is event-driven: `on_army_raised` spawns the icon/text/bg trio
//! and `on_army_dismiss` despawns them. `update` positions + draws.

use super::super::FONT_SIZE;
use super::common::UIWithArmy;
use crate::ecs::army::{ArmyLevy, ArmyName, ArmyOnLand};
use crate::ecs::land::LandHolding;
use crate::observers::{OnArmyDismiss, OnArmyRaised};
use bevy::color::Srgba;
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;

/// Marker on the entity whose world translation is the anchor for the army icon.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyIcon;

/// Back-references from the icon entity to its label children.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyIconLabelEntities {
    pub text: Entity,
    pub bg: Entity,
}

#[derive(Component)]
pub struct ArmyIconText;

#[derive(Component)]
pub struct ArmyIconLabelBg;

const BLADE_W: f32 = 4.0;
const BLADE_H: f32 = 30.0;
const CROSSGUARD_W: f32 = 16.0;
const CROSSGUARD_H: f32 = 3.0;
const GRIP_W: f32 = 3.0;
const GRIP_H: f32 = 6.0;
const POMMEL_W: f32 = 5.0;
const POMMEL_H: f32 = 4.0;
const SWORD_H: f32 = POMMEL_H + GRIP_H + CROSSGUARD_H + BLADE_H;
const LABEL_GAP: f32 = 6.0;
const LABEL_BG_Z: f32 = 0.9;

/// Draw the sword silhouette in `color` lines at world point `at` (the bottom-centre of the pommel).
pub fn draw(gizmos: &mut Gizmos, at: Vec2, color: Srgba) {
    let pommel_center_y = at.y + POMMEL_H / 2.0;
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, pommel_center_y)),
        Vec2::new(POMMEL_W, POMMEL_H),
        color,
    );
    let grip_bottom_y = at.y + POMMEL_H;
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, grip_bottom_y + GRIP_H / 2.0)),
        Vec2::new(GRIP_W, GRIP_H),
        color,
    );
    let crossguard_bottom_y = grip_bottom_y + GRIP_H;
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(
            at.x,
            crossguard_bottom_y + CROSSGUARD_H / 2.0,
        )),
        Vec2::new(CROSSGUARD_W, CROSSGUARD_H),
        color,
    );
    let blade_bottom_y = crossguard_bottom_y + CROSSGUARD_H;
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(at.x, blade_bottom_y + BLADE_H / 2.0)),
        Vec2::new(BLADE_W, BLADE_H),
        color,
    );
}

/// Spawn the `Text2d` and its black bg sprite at world point `anchor`.
fn spawn_label(commands: &mut Commands, text: &str, anchor: Vec2) -> (Entity, Entity) {
    let text_e = commands
        .spawn((
            Text2d::new(text.to_string()),
            TextFont::from_font_size(FONT_SIZE).with_font_weight(FontWeight::EXTRA_BOLD),
            TextColor(Color::Srgba(css::WHITE)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
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

/// Format the on-map label: `"Lannister Army (90)"`. Falls back to `"Army"` when no `ArmyName`.
fn format_label(name: Option<&ArmyName>, levy: &ArmyLevy) -> String {
    let name = name.map(|army_name| army_name.0.as_str()).unwrap_or("Army");
    format!("{name} ({})", levy.0)
}

/// Observer for `OnArmyRaised`: spawn the icon, text, and bg at the army's current land.
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

/// Observer for `OnArmyDismiss`: despawn the icon + text + bg trio.
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
        for e in [icon_e, label_ents.text, label_ents.bg] {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        return;
    }
}

/// Per-frame: position icon at the army's current land, draw the sword, refresh the label.
pub fn update(
    mut icons: Query<(&UIWithArmy, &mut Transform, &ArmyIconLabelEntities), With<ArmyIcon>>,
    mut text_q: Query<
        (&mut Transform, &mut Text2d, &TextLayoutInfo),
        (With<ArmyIconText>, Without<ArmyIcon>, Without<ArmyIconLabelBg>),
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
        icon_t.translation = pos.extend(icon_t.translation.z);
        draw(&mut gizmos, pos, css::WHITE);

        let Ok((mut text_t, mut text, layout)) = text_q.get_mut(label_ents.text) else {
            continue;
        };
        text.0 = format_label(army_name, army_levy);
        let text_anchor = Vec2::new(pos.x, pos.y + SWORD_H + LABEL_GAP);
        text_t.translation = text_anchor.extend(text_t.translation.z);

        if layout.size.x <= 0.0 || layout.size.y <= 0.0 {
            continue;
        }

        let Ok((mut sprite, mut bg_t)) = bg_q.get_mut(label_ents.bg) else {
            continue;
        };
        sprite.custom_size = Some(layout.size);
        bg_t.translation =
            Vec3::new(text_anchor.x, text_anchor.y + layout.size.y / 2.0, LABEL_BG_Z);
    }
}

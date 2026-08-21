//! Kingdom war rows — attacking wars then defending ones, each with its
//! begin date.

use crate::ecs::war::{WarBeginDate, WarName};
use crate::ecs::{KingdomHasWarsAttacking, KingdomHasWarsDefending};
use bevy::prelude::*;

use super::super::TITLE;

pub(super) fn render_wars_spans(world: &mut World, kingdom_e: Entity) -> Vec<(String, Color)> {
    let mut lines: Vec<String> = Vec::new();
    let mut wars = world.query::<(&WarName, &WarBeginDate)>();
    if let Some(attacking) = world.get::<KingdomHasWarsAttacking>(kingdom_e) {
        for war_e in attacking.iter() {
            if let Ok((name, begin)) = wars.get(world, war_e) {
                lines.push(format!("{} ({})", name.0, begin.0));
            }
        }
    }
    if let Some(defending) = world.get::<KingdomHasWarsDefending>(kingdom_e) {
        for war_e in defending.iter() {
            if let Ok((name, begin)) = wars.get(world, war_e) {
                lines.push(format!("[def] {} ({})", name.0, begin.0));
            }
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("wars:\n".to_string(), TITLE)];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((line.clone(), Color::WHITE));
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

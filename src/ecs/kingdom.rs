//! Kingdom entities: realms held by a leader character over a single land.
//!
//! The leader is defined by a `Courtier` of [`CourtierType::Ruler`](super::courtier::CourtierType::Ruler)
//! serving the kingdom; lookups go through
//! [`crate::helper::kingdom_helper::get_kingdom_ruler`]. There is no Bevy
//! relationship between kingdom and leader — the courtier IS the link.

use super::army::ArmyBelongsToKingdom;
use super::courtier::CourtierOfKingdom;
use super::war::{WarAttackerKingdom, WarDefenderKingdom};
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// Tags a kingdom entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct Kingdom;

/// A kingdom's display name. Seeded at populate from `Kingdom::name` (with
/// a `"Kingdom of <land>"` fallback) and rendered by the info panel.
#[derive(Component, Debug, Clone)]
pub struct KingdomName(pub String);

/// The land a kingdom holds. One-to-one: a kingdom holds at most one land (its capital).
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = super::land::LandHeldBy)]
pub struct KingdomHold(pub Entity);

/// The courtiers serving a kingdom — auto-maintained reverse of `CourtierOfKingdom`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfKingdom)]
pub struct KingdomHasCourtiers(Vec<Entity>);

/// The armies raised under this kingdom — auto-maintained reverse of `ArmyBelongsToKingdom`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ArmyBelongsToKingdom)]
pub struct KingdomHasArmies(Vec<Entity>);

/// The wars this kingdom is attacking in — auto-maintained reverse of `WarAttackerKingdom`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = WarAttackerKingdom)]
pub struct KingdomHasWarsAttacking(Vec<Entity>);

impl KingdomHasWarsAttacking {
    pub fn wars(&self) -> &[Entity] {
        &self.0
    }
}

/// The wars being fought against this kingdom — auto-maintained reverse of `WarDefenderKingdom`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = WarDefenderKingdom)]
pub struct KingdomHasWarsDefending(Vec<Entity>);

impl KingdomHasWarsDefending {
    pub fn wars(&self) -> &[Entity] {
        &self.0
    }
}
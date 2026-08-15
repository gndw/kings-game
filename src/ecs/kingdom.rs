//! Kingdom entities: realms held by a leader character over a single land.

use super::army::ArmyBelongsToKingdom;
use super::character::CharacterLeads;
use super::courtier::CourtierOfKingdom;
use super::war::{WarAttackerKingdom, WarDefenderKingdom};
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// Tags a kingdom entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct Kingdom;

/// The character who rules a kingdom. Bevy relationship; auto-maintains `CharacterLeads`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterLeads)]
pub struct KingdomLedBy(pub Entity);

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

/// The wars being fought against this kingdom — auto-maintained reverse of `WarDefenderKingdom`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = WarDefenderKingdom)]
pub struct KingdomHasWarsDefending(Vec<Entity>);

/// Marks a kingdom with no current ruler. Sits alongside the absence of
/// [`KingdomLedBy`] — the absence already encodes it, but the marker lets
/// systems and UI query the condition cleanly without scanning every kingdom.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct KingdomLeaderless;

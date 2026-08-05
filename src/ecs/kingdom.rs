//! Kingdom entities: realms held by a leader character across many lands.
//!
//! A kingdom carries the [`Kingdom`] marker, a [`KingdomLedBy`] link to its
//! ruler, a [`KingdomSeat`] pointing at its capital land, and a
//! [`KingdomHolds`] collection auto-maintained from each land's
//! [`LandHeldBy`](super::land::LandHeldBy).

use super::character::CharacterLeads;
use super::land::LandHeldBy;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// Tags a kingdom entity. A kingdom is otherwise just its relations.
#[derive(Component, Debug, Clone, Copy)]
pub struct Kingdom;

/// The character who rules a kingdom. Points at a
/// [`Character`](super::Character) entity. A Bevy relationship component:
/// inserting it auto-maintains [`CharacterLeads`] on the leader.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterLeads)]
pub struct KingdomLedBy(pub Entity);

/// The capital land of a kingdom. Points at a [`Land`](super::Land) entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct KingdomSeat(pub Entity);

/// The lands a kingdom holds — the auto-maintained reverse of
/// [`LandHeldBy`](super::land::LandHeldBy). Read-only: set [`LandHeldBy`] on
/// each land and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = LandHeldBy)]
pub struct KingdomHolds(Vec<Entity>);

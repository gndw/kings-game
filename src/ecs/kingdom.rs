//! Kingdom entities: realms held by a leader character over a single land.
//!
//! A kingdom carries the [`Kingdom`] marker, a [`KingdomLedBy`] link to its
//! ruler, and a [`KingdomHold`] link to the single land it holds. A kingdom's
//! held land is also its capital — 1 kingdom ↔ 1 land makes the seat implicit,
//! so there is no separate `KingdomSeat` component.

use super::character::CharacterLeads;
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

/// The land a kingdom holds. One-to-one: a kingdom holds at most one land,
/// which is also its capital. A Bevy relationship component: inserting it
/// auto-maintains [`LandHeldBy`](super::land::LandHeldBy) on the held land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = super::land::LandHeldBy)]
pub struct KingdomHold(pub Entity);

//! Kingdom entities: realms held by a leader character across many lands.
//!
//! A kingdom carries the [`Kingdom`] marker, a [`LedBy`] link to its ruler, a
//! [`Seat`] pointing at its capital land, and a [`Holds`] collection
//! auto-maintained from each land's [`HeldBy`].

use super::character::Leads;
use super::land::HeldBy;
use bevy::ecs::entity::Entity;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::{Component, Reflect};

/// Tags a kingdom entity. A kingdom is otherwise just its relations.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Kingdom;

/// The character who rules a kingdom. Points at a
/// [`Character`](super::Character) entity. A Bevy relationship component:
/// inserting it auto-maintains [`Leads`] on the leader.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
#[relationship(relationship_target = Leads)]
pub struct LedBy(pub Entity);

/// The capital land of a kingdom. Points at a [`Land`](super::Land) entity.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Seat(pub Entity);

/// The lands a kingdom holds — the auto-maintained reverse of [`HeldBy`].
/// Read-only: set [`HeldBy`] on each land and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component)]
#[relationship_target(relationship = HeldBy)]
pub struct Holds(Vec<Entity>);

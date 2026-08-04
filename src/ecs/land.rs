//! Land entities: the map's territories.
//!
//! A land carries the [`Land`] marker plus [`LandName`], [`LandBorders`],
//! [`LandHolding`], a [`HeldBy`] link to the kingdom that holds it, and a
//! [`BuildingsOn`] collection auto-maintained from each building's [`OnLand`].

use super::building::OnLand;
use super::kingdom::Holds;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A land. Name in [`LandName`], outline in [`LandBorders`], seat of power in
/// [`LandHolding`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Land;

/// A land's name.
#[derive(Component, Debug, Clone)]
pub struct LandName(pub String);

/// A land's polygon outline.
#[derive(Component, Debug, Clone)]
pub struct LandBorders(pub Vec<(f64, f64)>);

/// A land's seat of power.
#[derive(Component, Debug, Clone, Copy)]
pub struct LandHolding(pub (f64, f64));

/// The kingdom that holds a land. Points at a [`Kingdom`](super::Kingdom)
/// entity. A Bevy relationship component: inserting it auto-maintains
/// [`Holds`] on the kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = Holds)]
pub struct HeldBy(pub Entity);

/// The buildings standing in a land — the auto-maintained reverse of
/// [`OnLand`](super::building::OnLand). Read-only: set [`OnLand`] on each
/// building and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = OnLand)]
pub struct BuildingsOn(Vec<Entity>);

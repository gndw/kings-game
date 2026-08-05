//! Land entities: the map's territories.
//!
//! A land carries the [`Land`] marker plus [`LandName`], [`LandBorders`],
//! [`LandHolding`], a [`LandHeldBy`] link to the kingdom that holds it, and a
//! [`LandHasBuildings`] collection auto-maintained from each building's
//! [`BuildingOnLand`](super::building::BuildingOnLand).

use super::building::BuildingOnLand;
use super::kingdom::KingdomHolds;
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
/// [`KingdomHolds`] on the kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHolds)]
pub struct LandHeldBy(pub Entity);

/// The buildings standing in a land — the auto-maintained reverse of
/// [`BuildingOnLand`](super::building::BuildingOnLand). Read-only: set
/// [`BuildingOnLand`] on each building and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = BuildingOnLand)]
pub struct LandHasBuildings(Vec<Entity>);

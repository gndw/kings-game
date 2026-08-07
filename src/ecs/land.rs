//! Land entities: the map's territories.
//!
//! A land carries the [`Land`] marker plus [`LandName`], [`LandBorders`],
//! [`LandHolding`], a [`LandHeldBy`] link to the kingdom that holds it (auto-
//! maintained from the kingdom's [`KingdomHold`](super::kingdom::KingdomHold)),
//! and a [`LandHasBuildings`] collection auto-maintained from each building's
//! [`BuildingOnLand`](super::building::BuildingOnLand).

use super::army::ArmyOnLand;
use super::building::BuildingOnLand;
use super::kingdom::KingdomHold;
use super::marching::{MarchingFromLand, MarchingToLand};
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

/// The kingdom that holds a land — the auto-maintained reverse of
/// [`KingdomHold`](super::kingdom::KingdomHold). One-to-one: a land is held by
/// at most one kingdom. Read-only: set [`KingdomHold`] on a kingdom and Bevy's
/// hook keeps this in sync. The field is private (Bevy requires it for
/// `RelationshipTarget` correctness); read it via [`LandHeldBy::kingdom`].
#[derive(Component, Debug, Clone, Copy)]
#[relationship_target(relationship = KingdomHold)]
pub struct LandHeldBy(Entity);

impl LandHeldBy {
    /// The kingdom that holds this land.
    pub fn kingdom(&self) -> Entity {
        self.0
    }
}

/// The buildings standing in a land — the auto-maintained reverse of
/// [`BuildingOnLand`](super::building::BuildingOnLand). Read-only: set
/// [`BuildingOnLand`] on each building and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = BuildingOnLand)]
pub struct LandHasBuildings(Vec<Entity>);

/// The armies raised on this land — the auto-maintained reverse of
/// [`ArmyOnLand`](super::army::ArmyOnLand). Read-only: set `ArmyOnLand` on
/// each army and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ArmyOnLand)]
pub struct LandHasArmies(Vec<Entity>);

/// The marchings originating from this land — the auto-maintained reverse
/// of [`MarchingFromLand`](super::marching::MarchingFromLand). Read-only:
/// set `MarchingFromLand` on each marching and Bevy's hook keeps this in
/// sync. Not currently queried by gameplay code; the marching tick walks
/// `ArmyHasMarching` instead (which is keyed by the army). Lives here to
/// satisfy Bevy's `RelationshipTarget` correctness check.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingFromLand)]
pub struct LandHasMarchingsFrom(Vec<Entity>);

/// The marchings terminating at this land — the auto-maintained reverse of
/// [`MarchingToLand`](super::marching::MarchingToLand). Read-only: set
/// `MarchingToLand` on each marching and Bevy's hook keeps this in sync.
/// Same role as [`LandHasMarchingsFrom`] — here for the
/// `RelationshipTarget` contract.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingToLand)]
pub struct LandHasMarchingsTo(Vec<Entity>);

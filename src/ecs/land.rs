//! Land entities: the map's territories. `LandHeldBy`/`LandHasBuildings`/etc.
//! are auto-maintained reverses of the kingdom/building/etc. relationships.

use super::army::{ArmyControlsLand, ArmyOnLand};
use super::building::BuildingOnLand;
use super::kingdom::KingdomHold;
use super::marching::{MarchingFromLand, MarchingToLand};
use super::siege::SiegeDefenderLand;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A land. Name in `LandName`, outline in `LandBorders`, seat in `LandHolding`.
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

/// The kingdom that holds a land — auto-maintained reverse of `KingdomHold`. One-to-one.
#[derive(Component, Debug, Clone, Copy)]
#[relationship_target(relationship = KingdomHold)]
pub struct LandHeldBy(Entity);

impl LandHeldBy {
    pub fn kingdom(&self) -> Entity {
        self.0
    }
}

/// The buildings standing in a land — auto-maintained reverse of `BuildingOnLand`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = BuildingOnLand)]
pub struct LandHasBuildings(Vec<Entity>);

/// The armies raised on this land — auto-maintained reverse of `ArmyOnLand`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ArmyOnLand)]
pub struct LandHasArmies(Vec<Entity>);

/// Marchings originating from this land — auto-maintained reverse of `MarchingFromLand`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingFromLand)]
pub struct LandHasMarchingsFrom(Vec<Entity>);

/// Marchings terminating at this land — auto-maintained reverse of `MarchingToLand`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingToLand)]
pub struct LandHasMarchingsTo(Vec<Entity>);

/// Sieges being laid against this land — auto-maintained reverse of `SiegeDefenderLand`. Vec: multiple armies can siege the same land.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = SiegeDefenderLand)]
pub struct LandHasSiegesUnderAttack(Vec<Entity>);

/// The army currently controlling this land — auto-maintained reverse of `ArmyControlsLand`. Single: only one army can hold a conquered land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship_target(relationship = ArmyControlsLand)]
pub struct LandControlledByArmy(Entity);

impl LandControlledByArmy {
    pub fn army(&self) -> Entity {
        self.0
    }
}

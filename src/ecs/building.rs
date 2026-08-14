//! Building entities: the individual built structures standing in a land.

use super::land::LandHasBuildings;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;
use serde::Deserialize;

/// Per-instance operating state. Only `Active` contributes to yields.
#[derive(Component, Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub enum BuildingStatus {
    #[default]
    Active,
    Inactive,
    Building,
}

/// A built building instance. Stats come from the `BuildingDefs` roster via `BuildingOf`.
#[derive(Component, Debug, Clone, Copy)]
pub struct Building;

/// The current available levy pool. Replenished by `game::replenish_levy` up to the def's `levy`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BuildingLevy(pub u32);

/// Whether this building's levy has been raised to an army. The flag is
/// "is the levy currently sitting in an army?", not "is the pool empty?".
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BuildingIsRaised(pub bool);

/// The definition id this building is an instance of — a key into `BuildingDefs`. Not an entity link.
#[derive(Component, Debug, Clone)]
pub struct BuildingOf(pub String);

/// The land a building stands on. Bevy relationship; auto-maintains `LandHasBuildings`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasBuildings)]
pub struct BuildingOnLand(pub Entity);

/// The date the building becomes `Active` (only on `Building` buildings).
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingConstructionDate(pub Date);

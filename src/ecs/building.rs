//! Building entities: the individual built structures standing in a land.
//!
//! A building carries the [`Building`] marker, a [`BuildingOf`] link to its
//! definition (the read-only roster entry that holds its stats), and an
//! [`OnLand`] relationship to the land it stands on — whose reverse
//! [`BuildingsOn`](super::land::BuildingsOn) is auto-maintained.

use bevy::ecs::entity::Entity;
use bevy::prelude::Component;
use super::land::BuildingsOn;

/// A built building instance. No data of its own here: which kind of building it
/// is lives in [`BuildingOf`] (a definition id), and its stats are looked up in
/// the [`BuildingDefs`](crate::resources::buildings::BuildingDefs) roster.
#[derive(Component, Debug, Clone, Copy)]
pub struct Building;

/// The definition id this building is an instance of — a key into the
/// [`BuildingDefs`](crate::resources::buildings::BuildingDefs) resource. Not an
/// entity link, because definitions are a read-only roster, not entities.
#[derive(Component, Debug, Clone)]
pub struct BuildingOf(pub String);

/// The land a building stands on. Points at a [`Land`](super::Land) entity. A
/// Bevy relationship component: inserting it auto-maintains
/// [`BuildingsOn`](super::land::BuildingsOn) on the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = BuildingsOn)]
pub struct OnLand(pub Entity);

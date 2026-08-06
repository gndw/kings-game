//! Building entities: the individual built structures standing in a land.
//!
//! A building carries the [`Building`] marker, a [`BuildingOf`] link to its
//! definition (the read-only roster entry that holds its stats), and a
//! [`BuildingOnLand`] relationship to the land it stands on — whose reverse
//! [`LandHasBuildings`](super::land::LandHasBuildings) is auto-maintained.
//! [`BuildingStatus`] tracks whether it is `ACTIVE` (counted in yields),
//! `INACTIVE` (silently ignored), or `BUILDING` (under construction, finishing
//! at [`BuildingConstructionDate`]). Only `ACTIVE` buildings contribute to
//! `sum_kingdom_yield` and to the legend's yield column.

use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;
use super::land::LandHasBuildings;

/// Building is operating normally. Counts toward yield and total.
pub const BUILDING_STATUS_ACTIVE: u8 = 1;
/// Building exists but has been disabled (reserved for future code paths).
/// ponytail: defined but unused; the construct / destroy commands don't
/// toggle it. Wire it up if a "deactivate building" command ever ships.
pub const BUILDING_STATUS_INACTIVE: u8 = 2;
/// Building is under construction; flips to `ACTIVE` once the date passes
/// [`BuildingConstructionDate`].
pub const BUILDING_STATUS_BUILDING: u8 = 3;

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
/// [`LandHasBuildings`](super::land::LandHasBuildings) on the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasBuildings)]
pub struct BuildingOnLand(pub Entity);

/// Per-instance operating state. See [`BUILDING_STATUS_ACTIVE`] / `_INACTIVE` /
/// `_BUILDING`. Populated from content at spawn time and advanced to
/// `ACTIVE` by the `construction` system once the construction finishes.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingStatus(pub u8);

/// The date the building becomes `ACTIVE` (when present — only meaningful on
/// `BUILDING` buildings). Set at construction time as
/// `current_date + def.construction_time`; removed once the building flips to
/// `ACTIVE`. The legend shows this on `BUILDING` rows in place of yield.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingConstructionDate(pub Date);

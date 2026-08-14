//! Marching entities: queued orders to move an army along one road.
//!
//! One marching is one road: `MarchingFromLand`/`MarchingToLand` are its two
//! ends. An order to a land further away is a chain of marchings, one per
//! road on the traced route. The daily marching tick walks each army's
//! `ArmyHasMarching` queue, activating the hop whose source matches the
//! army's current land.

use super::army::ArmyHasMarching;
use super::land::{LandHasMarchingsFrom, LandHasMarchingsTo};
use super::road::RoadHasMarchings;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A marching order.
#[derive(Component, Debug, Clone, Copy)]
pub struct Marching;

/// The army this marching belongs to. Bevy relationship; auto-maintains `ArmyHasMarching` (Vec).
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ArmyHasMarching)]
pub struct MarchingArmy(pub Entity);

/// The land the army is marching from — one end of `MarchingOnRoad`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasMarchingsFrom)]
pub struct MarchingFromLand(pub Entity);

/// The land the army is marching to — the far end of `MarchingOnRoad`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasMarchingsTo)]
pub struct MarchingToLand(pub Entity);

/// The road this marching travels. One marching is exactly one road.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = RoadHasMarchings)]
pub struct MarchingOnRoad(pub Entity);

/// When the marching started. `None` while still `Scheduled`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MarchingBeginDate(pub Option<Date>);

/// When the army arrives at the target land. `None` while still `Scheduled`; set to `today + road_days`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MarchingArrivedDate(pub Option<Date>);

/// `Scheduled` (queued, dates empty) or `OnRoute` (active, dates populated).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarchingStatus {
    #[default]
    Scheduled,
    OnRoute,
}

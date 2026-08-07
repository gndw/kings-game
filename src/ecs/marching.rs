//! Marching entities: queued orders to move an army between lands.
//!
//! A marching is a separate entity kind from the army — it carries the
//! scheduling data (`MarchingFromLand`/`MarchingToLand`, `MarchingBeginDate`/
//! `MarchingArrivedDate`, `MarchingStatus`), while the army itself carries
//! the operational state ([`ArmyStatus`](super::army::ArmyStatus),
//! [`ArmyMarching`](super::army::ArmyMarching)). They are linked by a Bevy
//! relationship: [`MarchingArmy`] on the marching → the army it belongs to,
//! with [`ArmyHasMarching`](super::army::ArmyHasMarching) auto-maintained on
//! the army as a Vec (a queue, FIFO by `MarchingArmy` insertion order).
//!
//! Each marching has a status: `Scheduled` (queued, dates empty) or `OnRoute`
//! (activated, dates populated). The daily
//! [`march`](crate::game::marching::tick) tick walks every army, activates
//! scheduled marchings whose `MarchingFromLand` matches the army's current
//! land, and on the day the army arrives (today ≥ arrived date) moves the
//! army to the target land and activates the next scheduled marching in the
//! queue. When the queue runs dry the army returns to `Idle` and the
//! finished marching is despawned.

use super::army::ArmyHasMarching;
use super::land::{LandHasMarchingsFrom, LandHasMarchingsTo};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A marching order. The scheduling data lives in the other components on
/// this entity; the marker just tags the kind. Spawned by
/// [`MarchingArmy`](crate::commands::marching::MarchingArmy); reaped by the
/// daily marching tick when the army arrives at the target land.
#[derive(Component, Debug, Clone, Copy)]
pub struct Marching;

/// The army this marching belongs to. Bevy relationship: inserting it
/// auto-maintains [`ArmyHasMarching`] on the army (a Vec, so an army can
/// queue multiple marchings). The "current" marching is also held on the
/// army as [`ArmyMarching`](super::army::ArmyMarching) (single Entity, set
/// by the daily tick when the scheduled marching becomes `OnRoute`).
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ArmyHasMarching)]
pub struct MarchingArmy(pub Entity);

/// The land the army is marching from. Captured at marching-order time as
/// the army's current land; the daily tick only activates a scheduled
/// marching when the army is sitting on this land (the "begin on where the
/// army land is" rule). Bevy relationship: auto-maintains
/// [`LandHasMarchingsFrom`] on the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasMarchingsFrom)]
pub struct MarchingFromLand(pub Entity);

/// The land the army is marching to. The daily tick moves the army's
/// `ArmyOnLand` to this land on the arrived date. Bevy relationship:
/// auto-maintains [`LandHasMarchingsTo`] on the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasMarchingsTo)]
pub struct MarchingToLand(pub Entity);

/// When the marching started. `None` while still `Scheduled`; set by the
/// daily tick when activating the marching to `today`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MarchingBeginDate(pub Option<Date>);

/// When the army arrives at the target land. `None` while still `Scheduled`;
/// set by the daily tick when activating the marching to
/// `today + 14 days`. The tick uses this to decide when the army moves.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MarchingArrivedDate(pub Option<Date>);

/// The marching's state. `Scheduled` — queued, dates empty, waiting for the
/// army to be on the right land. `OnRoute` — active, the army is marching
/// and the dates are populated. The daily tick flips a `Scheduled` marching
/// to `OnRoute` when activating it. Serialized by variant name (the
/// `BuildingStatus`](super::building::BuildingStatus) one-field-per-state
/// decision).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarchingStatus {
    #[default]
    Scheduled,
    OnRoute,
}

//! Army entities: a levy raised on a land, belonging to a kingdom.
//!
//! Sits inside `ecs/` alongside every other entity kind — the commands and UI
//! consume it through the normal `crate::ecs::army::*` path. The companion
//! reverse components live next to the entity they sit on, per the
//! relationship-colocation decision — `LandHasArmies` in `super::land`,
//! `KingdomHasArmies` in `super::kingdom`.
//!
//! An army carries [`Army`] (marker), [`ArmyLevy`] (the troop count), an
//! [`ArmyOnLand`] relationship to its land, and an [`ArmyBelongsToKingdom`]
//! relationship to its kingdom. Both relationships are Bevy-native, hook-
//! maintained — despawning the army auto-pulls it out of the land's
//! `LandHasArmies` and the kingdom's `KingdomHasArmies`.

use super::kingdom::KingdomHasArmies;
use super::land::LandHasArmies;
use super::marching::MarchingArmy;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A raised army. Troops in [`ArmyLevy`], position in
/// [`ArmyOnLand`](self::ArmyOnLand), realm in
/// [`ArmyBelongsToKingdom`](self::ArmyBelongsToKingdom).
#[derive(Component, Debug, Clone, Copy)]
pub struct Army;

/// The operational status of an army. `Idle` — sitting on its land, ready for
/// orders. `Marching` — currently in motion, working through a queue of
/// marchings. Set to `Idle` on raise; the daily
/// [`march`](crate::game::marching::tick) tick flips it to `Marching` when a
/// scheduled marching is activated, and back to `Idle` when the queue runs
/// dry.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArmyStatus {
    #[default]
    Idle,
    Marching,
}

/// The number of levy troops in this army. Defaults to 0; future code paths
/// (the `levy_rate` on military buildings, the player's [`CharacterLevy`]
/// pool) will fill this in.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ArmyLevy(pub u64);

/// The land this army is raised on. Bevy relationship: inserting it auto-
/// maintains `LandHasArmies` on the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasArmies)]
pub struct ArmyOnLand(pub Entity);

/// The kingdom this army belongs to. Bevy relationship: inserting it auto-
/// maintains `KingdomHasArmies` on the kingdom. Set explicitly on raise (not
/// derived through the land's `LandHeldBy`) so the link survives a future
/// world where kingdoms can hold multiple lands.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasArmies)]
pub struct ArmyBelongsToKingdom(pub Entity);

/// The display name of this army. Set at raise time to the leader's house +
/// `" Army"` (e.g. `"Lannister Army"`); read by both the on-map indicator
/// and the right-hand `ARMY` panel. A mod can rename via a future command.
#[derive(Component, Debug, Clone)]
pub struct ArmyName(pub String);

/// The marching this army is currently executing. Only present when
/// `ArmyStatus == Marching`; the daily
/// [`march`](crate::game::marching::tick) tick inserts it when activating a
/// scheduled marching and removes it when the queue runs dry. Read by the
/// tick to advance the army's `ArmyOnLand` and dequeue the next marching.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyMarching(pub Entity);

/// The marchings queued against this army — the auto-maintained reverse of
/// [`MarchingArmy`](super::marching::MarchingArmy). Includes both the
/// currently-executing marching and any scheduled ones waiting in the queue.
/// The marching tick walks this to find the next scheduled marching whose
/// `MarchingFromLand` matches the army's current land.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingArmy)]
pub struct ArmyHasMarching(Vec<Entity>);
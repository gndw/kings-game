//! Army entities: a levy raised on a land, belonging to a kingdom.
//!
//! Bevy relationships: `ArmyOnLand` ↔ `LandHasArmies`,
//! `ArmyBelongsToKingdom` ↔ `KingdomHasArmies`, `ArmyHasMarching` ↔ `MarchingArmy`,
//! `ArmyHasSiege` ↔ `SiegeAttackerArmy`, `ArmyControlsLand` ↔ `LandControlledByArmy`.

use super::kingdom::KingdomHasArmies;
use super::land::{LandControlledByArmy, LandHasArmies};
use super::marching::MarchingArmy;
use super::siege::SiegeAttackerArmy;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A raised army.
#[derive(Component, Debug, Clone, Copy)]
pub struct Army;

/// The operational status of an army.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArmyStatus {
    #[default]
    Idle,
    /// Being mustered; the per-day raising tick grows `ArmyLevy` until it reaches `ArmyMaxLevy`.
    Raising,
    /// In motion; the marching tick advances it through `ArmyHasMarching`.
    Marching,
    /// Committed to a siege on the land it stands on.
    Sieging,
}

/// Current levy in this army. Grows via `game::raising_army::on_day` until it reaches `ArmyMaxLevy`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ArmyLevy(pub u64);

/// Levy the army will have once mustering finishes — snapshotted at raise time.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ArmyMaxLevy(pub u64);

/// The land this army is raised on. Bevy relationship.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasArmies)]
pub struct ArmyOnLand(pub Entity);

/// The kingdom this army belongs to. Set explicitly on raise, not derived through the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasArmies)]
pub struct ArmyBelongsToKingdom(pub Entity);

/// Display name, e.g. `"Aurelan Army"`. Set at raise from the leader's house.
#[derive(Component, Debug, Clone)]
pub struct ArmyName(pub String);

/// The marching this army is currently executing. Only present when `ArmyStatus == Marching`.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArmyMarching(pub Entity);

/// Marchings queued against this army — the auto-maintained reverse of `MarchingArmy`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MarchingArmy)]
pub struct ArmyHasMarching(Vec<Entity>);

/// The siege this army is currently running — auto-maintained reverse of `SiegeAttackerArmy`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship_target(relationship = SiegeAttackerArmy)]
pub struct ArmyHasSiege(Entity);

impl ArmyHasSiege {
    pub fn siege(&self) -> Entity {
        self.0
    }
}

/// The land this army controls. Set by the siege tick on a 100% resolution.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandControlledByArmy)]
pub struct ArmyControlsLand(pub Entity);

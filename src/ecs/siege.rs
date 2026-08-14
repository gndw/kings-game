//! Siege entities: an army besieging a land. A siege is a separate entity
//! kind from the army and the land — it carries the progress and schedule of
//! the assault.

use super::army::ArmyHasSiege;
use super::land::LandHasSiegesUnderAttack;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A siege in progress.
#[derive(Component, Debug, Clone, Copy)]
pub struct Siege;

/// The army laying the siege. Bevy relationship; auto-maintains `ArmyHasSiege`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ArmyHasSiege)]
pub struct SiegeAttackerArmy(pub Entity);

/// The land being besieged. Bevy relationship; auto-maintains `LandHasSiegesUnderAttack`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasSiegesUnderAttack)]
pub struct SiegeDefenderLand(pub Entity);

/// How far the siege has progressed, 0–100. `100` means the siege is won.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SiegeProgress(pub u32);

/// The next day the per-day siege tick should resolve an event for this siege.
#[derive(Component, Debug, Clone, Copy)]
pub struct SiegeNextEventDate(pub Date);

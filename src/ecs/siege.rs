//! Siege entities: an army besieging a land.
//!
//! A siege is a separate entity kind from the army and the land — it carries
//! the *progress* ([`SiegeProgress`]) and the *schedule* ([`SiegeNextEventDate`])
//! of the assault, while the army carries its operational state and the land
//! is the target. The two end-points are linked through Bevy relationships:
//!
//! - [`SiegeAttackerArmy`] (on siege) ↔ [`ArmyHasSiege`](super::army::ArmyHasSiege)
//!   (on army, single — an army can only besiege one land at a time).
//! - [`SiegeDefenderLand`] (on siege) ↔ [`LandHasSiegesUnderAttack`](super::land::LandHasSiegesUnderAttack)
//!   (on land, `Vec` — multiple armies can besiege the same land).
//!
//! Spawned by [`crate::commands::lay_siege::LaySiege`] on the player's pick of an
//! army that's standing on a foreign land. The per-day
//! [`tick`](crate::game::siege::tick) advances `SiegeProgress` on each
//! scheduled event; at 100% the siege is won — `ArmyControlsLand` lands on
//! the army, every standing building on the land is set to `Inactive`, and
//! the siege despawns (with the army returning to `Idle`).
//!
//! No resolution path for the defender kingdom yet — the land's
//! `LandHeldBy` stays on the original holder. That's the next change
//! (conquest transfer / peace); the relationship graph is already wired so
//! adding it is additive on the existing archetypes.

use super::army::ArmyHasSiege;
use super::land::LandHasSiegesUnderAttack;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A siege in progress. The two ends are in [`SiegeAttackerArmy`] and
/// [`SiegeDefenderLand`]; the progress + schedule are in
/// [`SiegeProgress`] / [`SiegeNextEventDate`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Siege;

/// The army laying the siege. Bevy relationship: inserting it auto-
/// maintains [`ArmyHasSiege`](super::army::ArmyHasSiege) on the army.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = ArmyHasSiege)]
pub struct SiegeAttackerArmy(pub Entity);

/// The land being besieged. Bevy relationship: inserting it auto-maintains
/// [`LandHasSiegesUnderAttack`](super::land::LandHasSiegesUnderAttack) on
/// the land.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = LandHasSiegesUnderAttack)]
pub struct SiegeDefenderLand(pub Entity);

/// How far the siege has progressed, 0–100. `100` means the siege is won
/// — the next tick despawns the siege, flips the buildings to `Inactive`,
/// and inserts [`ArmyControlsLand`](super::army::ArmyControlsLand) on the
/// attacking army. Bumped by `+30` per scheduled event in
/// [`tick`](crate::game::siege::tick), capped at `100`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SiegeProgress(pub u32);

/// The next day the per-day siege tick should resolve an event for this
/// siege. The tick advances this by 10 days after each event, so a siege
/// takes 4 events (~30 days at the base calendar) to reach 100% — 30 + 30
/// + 30 + 10 → cap. The tick fires on `today >= next` so the very first
/// event lands 10 days after the siege is declared.
#[derive(Component, Debug, Clone, Copy)]
pub struct SiegeNextEventDate(pub Date);

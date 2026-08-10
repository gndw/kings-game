//! Character entities: the people of the world.
//!
//! A character carries the [`Character`] marker plus [`CharacterName`],
//! [`CharacterDateOfBirth`], [`CharacterGold`], [`CharacterLevy`],
//! [`CharacterGoldYield`], a [`CharacterOfHouse`] link to their house, a
//! [`CharacterLeads`] link to the kingdom they rule (if any), and the
//! auto-maintained reverse [`CharacterHasCourtiers`] for O(1) read of who
//! serves at their court.

use super::courtier::CourtierOfCharacter;
use super::kingdom::KingdomLedBy;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A person. Their house is [`CharacterOfHouse`]; name in [`CharacterName`],
/// date of birth in [`CharacterDateOfBirth`], treasury in [`CharacterGold`],
/// troops in [`CharacterLevy`], monthly yield in [`CharacterGoldYield`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Character;

/// A character's name.
#[derive(Component, Debug, Clone)]
pub struct CharacterName(pub String);

/// Which house a character belongs to. Points at a [`House`](super::House)
/// entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterOfHouse(pub Entity);

/// A character's date of birth, in the world's calendar. The years-elapsed
/// "age" is derived against the current date — see [`crate::game::age`].
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterDateOfBirth(pub Date);

/// A character's treasury. Signed: a ruler can be in debt.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGold(pub i64);

/// A character's available troops.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterLevy(pub u64);

/// A character's monthly gold yield (income less upkeep). Signed: a realm can
/// run at a loss.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGoldYield(pub i64);

/// The kingdoms a character leads — the auto-maintained reverse of
/// [`KingdomLedBy`]. **Many-to-many** (`Vec<Entity>`): a character can lead
/// any number of kingdoms simultaneously. The conquest-transfer flow is
/// built on this — a player who enforces a `Take` demand on a kingdom keeps
/// their original realm and adds the conquered one to their list.
///
/// Bevy's hook keeps this in sync: setting [`KingdomLedBy`] on a kingdom
/// adds the leader here; removing it drops the entry. Callers that want
/// "pick one kingdom" should call `.first().copied()` (the war-declare
/// command and the ctx startup pick the first); callers that want "any of
/// the character's kingdoms satisfies X" should use `.iter().any(...)`;
/// callers that want "walk all kingdoms" use `.iter()` directly.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = KingdomLedBy)]
pub struct CharacterLeads(Vec<Entity>);

impl CharacterLeads {
    /// The kingdoms this character leads (empty slice if none).
    pub fn kingdoms(&self) -> &[Entity] {
        &self.0
    }
}

/// The courtiers serving a character — the auto-maintained reverse of
/// [`CourtierOfCharacter`]. Read-only: set [`CourtierOfCharacter`] on the
/// courtier and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfCharacter)]
pub struct CharacterHasCourtiers(Vec<Entity>);

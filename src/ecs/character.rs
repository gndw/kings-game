//! Character entities: the people of the world.
//!
//! A character carries the [`Character`] marker plus [`CharacterName`],
//! [`CharacterAge`], [`CharacterGold`], [`CharacterLevy`],
//! [`CharacterGoldYield`], a [`CharacterOfHouse`] link to their house, a
//! [`CharacterLeads`] link to the kingdom they rule (if any), and the
//! auto-maintained reverse [`CharacterHasCourtiers`] for O(1) read of who
//! serves at their court.

use super::courtier::CourtierOfCharacter;
use super::kingdom::KingdomLedBy;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A person. Their house is [`CharacterOfHouse`]; name in [`CharacterName`],
/// age in [`CharacterAge`], treasury in [`CharacterGold`], troops in
/// [`CharacterLevy`], monthly yield in [`CharacterGoldYield`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Character;

/// A character's name.
#[derive(Component, Debug, Clone)]
pub struct CharacterName(pub String);

/// Which house a character belongs to. Points at a [`House`](super::House)
/// entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterOfHouse(pub Entity);

/// A character's age, in years.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterAge(pub u32);

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

/// The kingdom a character leads — the auto-maintained reverse of
/// [`KingdomLedBy`], for O(1) character→kingdom lookup. Read-only: set
/// [`KingdomLedBy`] on the kingdom and Bevy's hook keeps this in sync.
///
/// One-to-one (single `Entity`): a character leads at most one kingdom. If a
/// second kingdom claims the same leader, Bevy drops the older [`KingdomLedBy`].
#[derive(Component, Debug)]
#[relationship_target(relationship = KingdomLedBy)]
pub struct CharacterLeads(Entity);

impl CharacterLeads {
    /// The kingdom this character leads.
    pub fn kingdom(&self) -> Entity {
        self.0
    }
}

/// The courtiers serving a character — the auto-maintained reverse of
/// [`CourtierOfCharacter`]. Read-only: set [`CourtierOfCharacter`] on the
/// courtier and Bevy's hook keeps this in sync.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfCharacter)]
pub struct CharacterHasCourtiers(Vec<Entity>);

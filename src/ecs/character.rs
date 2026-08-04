//! Character entities: the people of the world.
//!
//! A character carries the [`Character`] marker plus [`CharacterName`],
//! [`CharacterAge`], [`CharacterGold`], [`CharacterLevy`],
//! [`CharacterGoldYield`], a [`HouseOf`] link to their house, and — if they
//! rule — a [`Leads`] link to their kingdom.

use super::kingdom::LedBy;
use bevy::ecs::entity::Entity;
use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::{Component, Reflect};

/// A person. Their house is [`HouseOf`]; name in [`CharacterName`], age in
/// [`CharacterAge`], treasury in [`CharacterGold`], troops in [`CharacterLevy`],
/// monthly yield in [`CharacterGoldYield`].
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct Character;

/// A character's name.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct CharacterName(pub String);

/// Which house a character belongs to. Points at a [`House`](super::House) entity.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct HouseOf(pub Entity);

/// A character's age, in years.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct CharacterAge(pub u32);

/// A character's treasury. Signed: a ruler can be in debt.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct CharacterGold(pub i64);

/// A character's available troops.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct CharacterLevy(pub u64);

/// A character's monthly gold yield (income less upkeep). Signed: a realm can
/// run at a loss.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct CharacterGoldYield(pub i64);

/// The kingdom a character leads — the auto-maintained reverse of [`LedBy`],
/// for O(1) character→kingdom lookup. Read-only: set [`LedBy`] on the kingdom
/// and Bevy's hook keeps this in sync.
///
/// One-to-one (single `Entity`): a character leads at most one kingdom. If a
/// second kingdom claims the same leader, Bevy drops the older [`LedBy`].
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
#[relationship_target(relationship = LedBy)]
pub struct Leads(Entity);

impl Leads {
    /// The kingdom this character leads.
    pub fn kingdom(&self) -> Entity {
        self.0
    }
}

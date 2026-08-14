//! Character entities: the people of the world.

use super::courtier::CourtierOfCharacter;
use super::kingdom::KingdomLedBy;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;
use serde::Deserialize;

/// A person.
#[derive(Component, Debug, Clone, Copy)]
pub struct Character;

/// A character's sex. Authored as `"m"` / `"f"`.
#[derive(Component, Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub enum CharacterSex {
    #[default]
    #[serde(rename = "m")]
    Male,
    #[serde(rename = "f")]
    Female,
}

/// A character's name.
#[derive(Component, Debug, Clone)]
pub struct CharacterName(pub String);

/// Which house a character belongs to.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterOfHouse(pub Entity);

/// A character's date of birth in the world's calendar.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterDateOfBirth(pub Date);

/// A character's treasury. Signed: a ruler can be in debt.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGold(pub i64);

/// A character's available troops.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterLevy(pub u64);

/// A character's monthly gold yield (income less upkeep). Signed: a realm can run at a loss.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGoldYield(pub i64);

/// The kingdoms a character leads — the auto-maintained reverse of `KingdomLedBy`.
/// Many-to-many: a character can lead several kingdoms simultaneously.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = KingdomLedBy)]
pub struct CharacterLeads(Vec<Entity>);

impl CharacterLeads {
    pub fn kingdoms(&self) -> &[Entity] {
        &self.0
    }
}

/// The courtiers serving a character — the auto-maintained reverse of `CourtierOfCharacter`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfCharacter)]
pub struct CharacterHasCourtiers(Vec<Entity>);

/// A character's father. Bevy relationship; auto-maintains `CharacterHasFatheredChildren`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasFatheredChildren)]
pub struct CharacterHasFather(pub Entity);

/// A character's mother. Bevy relationship; auto-maintains `CharacterHasMotheredChildren`.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasMotheredChildren)]
pub struct CharacterHasMother(pub Entity);

/// The children a character has fathered — auto-maintained reverse of `CharacterHasFather`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CharacterHasFather)]
pub struct CharacterHasFatheredChildren(Vec<Entity>);

impl CharacterHasFatheredChildren {
    pub fn children(&self) -> &[Entity] {
        &self.0
    }
}

/// The children a character has borne — auto-maintained reverse of `CharacterHasMother`.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CharacterHasMother)]
pub struct CharacterHasMotheredChildren(Vec<Entity>);

impl CharacterHasMotheredChildren {
    pub fn children(&self) -> &[Entity] {
        &self.0
    }
}

/// A character's husband. Bevy relationship on the wife; auto-maintains
/// `CharacterHasWife` on the husband.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasWife)]
pub struct CharacterHasHusband(pub Entity);

/// A husband's wife — one-to-one target of `CharacterHasHusband`. Sits on the husband.
#[derive(Component, Debug, Clone, Copy)]
#[relationship_target(relationship = CharacterHasHusband)]
pub struct CharacterHasWife(Entity);

impl CharacterHasWife {
    pub fn wife(&self) -> Entity {
        self.0
    }
}

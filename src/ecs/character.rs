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

/// A character's gender. Authored as `"m"` / `"f"`.
#[derive(Component, Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub enum CharacterGender {
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

/// Whether a character is alive. Defaults to `true`; flips to `false` on death
/// (along with [`CharacterDateOfDeath`]).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterIsAlive(pub bool);

/// When a character died — `None` while alive, the date of death once
/// [`CharacterIsAlive`] flips to `false`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterDateOfDeath(pub Option<Date>);

/// The next simulated date this character is due for a death-check roll.
/// Set from content on load; updated by the aging/death system after each
/// surviving roll.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterNextDeathEventDate(pub Date);

/// A character's treasury. Signed: a ruler can be in debt.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGold(pub i64);

/// A character's available troops.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterLevy(pub u64);

/// A character's monthly gold yield (income less upkeep). Signed: a realm can run at a loss.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterGoldYield(pub i64);

/// A character's martial skill (0..=20) — field command, battle tactics,
/// siegecraft. Folded with logistics: drives monthly levy replenishment,
/// march distance, and army combat power.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterMartial(pub i32);

/// A character's prowess (0..=20) — personal combat, duels, ambushes,
/// surviving assassination. Drives the monthly personal safety check.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterProwess(pub i32);

/// A character's treasury skill (0..=20) — tax efficiency plus trade
/// leverage. Drives the monthly gold yield multiplier on the realm.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterTreasury(pub i32);

/// A character's prudence (0..=20) — internal judgment and external accord.
/// Drives monthly vassal + foreign opinion drift.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterPrudence(pub i32);

/// A character's intrigue (0..=20) — plots, detection, secrets. Drives
/// monthly plot-detection threshold and rumor spread.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterIntrigue(pub i32);

/// A character's faith (0..=20) — piety plus theological literacy. Drives
/// monthly Church favor drift, legitimacy drift, and event-tier unlocks.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterFaith(pub i32);

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

/// What kind of memory this is — drives the opinion contribution. New variants
/// (AttackedBy, DefendedBy, ...) plug into [`opinion_helper`] without
/// requiring new components on the Memory entity.
#[derive(Component, Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum MemoryKind {
    ReceivedGold { amount: i64 },
}

/// Marker for a memory entity. One memory = one entity, hanging off its
/// recipient character via [`MemoryOfCharacter`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Memory;

/// Who OWNS this memory — the character who experienced it. Bevy relationship;
/// auto-maintains [`CharacterHasMemories`] on the recipient.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasMemories)]
pub struct MemoryOfCharacter(pub Entity);

/// The memories a character carries — auto-maintained reverse of
/// [`MemoryOfCharacter`].
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = MemoryOfCharacter)]
pub struct CharacterHasMemories(Vec<Entity>);

impl CharacterHasMemories {
    pub fn memories(&self) -> &[Entity] {
        &self.0
    }
}

/// Who the memory is ABOUT — the actor whose deed is remembered (e.g. the
/// giver of a gift). Not a relationship; we walk from owner to targets.
#[derive(Component, Debug, Clone, Copy)]
pub struct MemoryTowardCharacter(pub Entity);

/// When this memory was created.
#[derive(Component, Debug, Clone, Copy)]
pub struct MemoryCreatedDate(pub Date);

/// When this memory expires — set at creation to `created + duration_days`,
/// then despawned by [`crate::game::remembering::on_day`].
#[derive(Component, Debug, Clone, Copy)]
pub struct MemoryUntilDate(pub Date);

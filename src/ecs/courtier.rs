//! Court appointments linking a character to a kingdom.
//!
//! The courtier entity carries [`Courtier`] + [`CourtierType`] and the two
//! relationship components [`CourtierOfCharacter`] (to the character served)
//! and [`CourtierOfKingdom`] (to the kingdom served). Their auto-maintained
//! reverses — `CharacterHasCourtiers` and `KingdomHasCourtiers` — live in
//! [`super::character`] and [`super::kingdom`] respectively, alongside the
//! entity each side sits on.

use super::character::CharacterHasCourtiers;
use super::kingdom::KingdomHasCourtiers;
use bevy::prelude::{Component, Entity};
use serde::Deserialize;

#[derive(Component, Debug, Clone, Copy)]
pub struct Courtier;

/// Court role. Add variants as roles become playable.
#[derive(Component, Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum CourtierType {
    Courtier,
    /// The character who rules the kingdom they serve — defines
    /// "who leads this kingdom" at the data layer. Looked up via
    /// [`crate::helper::kingdom_helper::get_kingdom_ruler`].
    Ruler,
}

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasCourtiers)]
pub struct CourtierOfCharacter(pub Entity);

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasCourtiers)]
pub struct CourtierOfKingdom(pub Entity);

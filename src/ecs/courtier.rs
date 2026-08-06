//! Court appointments linking a character to a kingdom.

use bevy::prelude::{Component, Entity};
use serde::Deserialize;

#[derive(Component, Debug, Clone, Copy)]
pub struct Courtier;

/// Court role. Add variants as roles become playable.
#[derive(Component, Debug, Clone, Copy, Deserialize)]
pub enum CourtierType {
    Courtier,
}

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CharacterHasCourtiers)]
pub struct CourtierOfCharacter(pub Entity);

#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfCharacter)]
pub struct CharacterHasCourtiers(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasCourtiers)]
pub struct CourtierOfKingdom(pub Entity);

#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CourtierOfKingdom)]
pub struct KingdomHasCourtiers(Vec<Entity>);

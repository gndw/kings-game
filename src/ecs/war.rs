//! War entities: a declared state of hostility between two kingdoms over a
//! casus belli and a list of demands.
//!
//! `KingdomHasWarsAttacking`/`KingdomHasWarsDefending` live in `super::kingdom`
//! (relationship-colocation rule).

use super::kingdom::{KingdomHasWarsAttacking, KingdomHasWarsDefending};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A declared war.
#[derive(Component, Debug, Clone, Copy)]
pub struct War;

/// The kingdom that declared the war — the attacker. Bevy relationship.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasWarsAttacking)]
pub struct WarAttackerKingdom(pub Entity);

/// The kingdom the war is fought against — the defender. Bevy relationship.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasWarsDefending)]
pub struct WarDefenderKingdom(pub Entity);

/// The casus belli (the *shape* of the fight). `Conquest` is the only variant today.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WarCasusBelliType {
    #[default]
    Conquest = 1,
}

/// The shape of a single demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarDemandType {
    Take = 1,
}

/// One concrete demand a war is fought over.
#[derive(Debug, Clone, Copy)]
pub struct WarDemand {
    pub demand_type: WarDemandType,
    pub target: Entity,
}

/// The list of demands a war is fought over. Sits on the war entity.
#[derive(Component, Debug, Clone, Default)]
pub struct WarDemands(pub Vec<WarDemand>);

/// Human-readable label, e.g. `"Conquest over Kingdom of Crossford"`. Set at declare time.
#[derive(Component, Debug, Clone)]
pub struct WarName(pub String);

/// The date the war was declared. Snapshot at declare time.
#[derive(Component, Debug, Clone, Copy)]
pub struct WarBeginDate(pub Date);

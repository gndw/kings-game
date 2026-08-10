//! Casus belli entities: the *reason* a war was declared.
//!
//! A casus belli is a separate entity kind from the war that uses it, linked
//! to its [`WarWithCasusBelli`](super::war::WarWithCasusBelli) by a Bevy
//! relationship. The CB itself carries one target — the kingdom being
//! claimed — via [`CasusBelliKingdom`]. The CB type ([`CasusBelliType`]) is
//! the *what is being claimed*; only `Conquest` (seize the target kingdom)
//! exists today, more land-grab shapes are additive.
//!
//! Pairing with [`super::kingdom`]: the reverse [`KingdomHasCasusBelli`]
//! lives there because that is where it sits. The reverse
//! [`CasusBelliOnWar`] lives here because that is where it sits — same
//! relationship-colocation rule as every other Bevy relationship in this
//! codebase.

use super::kingdom::KingdomHasCasusBelli;
use super::war::WarWithCasusBelli;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A casus belli. The reason lives in [`CasusBelliType`]; the target kingdom
/// in [`CasusBelliKingdom`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CasusBelli;

/// The shape of a casus belli — what declaring war under this CB seeks to
/// achieve. Only `Conquest` exists for now: a Conquest CB names a target
/// kingdom, and the war it backs aims to hand that kingdom to the attacker
/// when the war resolves. Variant names serialize (the
/// `BuildingStatus`](super::building::BuildingStatus) convention).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CasusBelliType {
    #[default]
    Conquest,
}

/// The kingdom a casus belli targets — the realm being claimed. For
/// [`CasusBelliType::Conquest`] this is the kingdom the war aims to seize.
/// Bevy relationship: inserting it auto-maintains
/// [`KingdomHasCasusBelli`](super::kingdom::KingdomHasCasusBelli) on the
/// target kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasCasusBelli)]
pub struct CasusBelliKingdom(pub Entity);

/// The wars that carry this casus belli — the auto-maintained reverse of
/// [`WarWithCasusBelli`]. Lives here because that is where it sits. Not
/// currently queried by gameplay code; included so Bevy's
/// `RelationshipTarget` correctness check passes.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = WarWithCasusBelli)]
pub struct CasusBelliOnWar(Vec<Entity>);

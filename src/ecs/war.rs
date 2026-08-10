//! War entities: a declared state of hostility between two kingdoms over a
//! casus belli.
//!
//! A war is a separate entity kind — it links two kingdoms (attacker +
//! defender) and one casus belli through Bevy relationships, with the
//! relationship hooks maintaining the reverses on the kingdom
//! ([`KingdomHasWarsAttacking`], [`KingdomHasWarsDefending`],
//! [`KingdomHasCasusBelli`](super::kingdom::KingdomHasCasusBelli)) and on
//! the CB ([`CasusBelliOnWar`](super::casus_belli::CasusBelliOnWar)).
//!
//! Spawned by [`crate::commands::declare_war::DeclareWar`] on the player's
//! pick of defender kingdom and CB type. The war has no status / no tick /
//! no resolution yet — those land later. Today the entity is just the link
//! graph; the chronicle line in `DeclareWar` is the only observable.
//!
//! Pairing with [`super::kingdom`]: the reverse targets
//! (`KingdomHasWarsAttacking`, `KingdomHasWarsDefending`) live there because
//! that is where they sit — the relationship-colocation rule. Pairing with
//! [`super::casus_belli`]: same pattern, reverse lives on the CB.

use super::casus_belli::CasusBelliOnWar;
use super::kingdom::{KingdomHasWarsAttacking, KingdomHasWarsDefending};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A declared war. The two belligerents are in [`WarAttackerKingdom`] and
/// [`WarDefenderKingdom`]; the *why* is in [`WarWithCasusBelli`].
#[derive(Component, Debug, Clone, Copy)]
pub struct War;

/// The kingdom that declared the war — the attacker. Bevy relationship:
/// inserting it auto-maintains [`KingdomHasWarsAttacking`] on the kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasWarsAttacking)]
pub struct WarAttackerKingdom(pub Entity);

/// The kingdom the war is fought against — the defender. Bevy relationship:
/// inserting it auto-maintains [`KingdomHasWarsDefending`] on the kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = KingdomHasWarsDefending)]
pub struct WarDefenderKingdom(pub Entity);

/// The casus belli backing this war — the *reason* it was declared. Bevy
/// relationship: inserting it auto-maintains
/// [`CasusBelliOnWar`](super::casus_belli::CasusBelliOnWar) on the CB.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = CasusBelliOnWar)]
pub struct WarWithCasusBelli(pub Entity);

/// The human-readable label of this war. Set at declare time by
/// [`crate::commands::declare_war`] — format depends on the CB type, e.g.
/// `"Conquest over Kingdom of Riverrun"` for a Conquest CB. Read by the
/// right-hand `WARS` panel to name each war.
#[derive(Component, Debug, Clone)]
pub struct WarName(pub String);

/// The date the war was declared — a snapshot of [`Date`] at the moment
/// [`crate::commands::declare_war`] spawned the war. Stored on the war
/// rather than read live so the panel can show "declared on YYYY.MM.DD"
/// without re-running the war-declare snapshot.
#[derive(Component, Debug, Clone, Copy)]
pub struct WarBeginDate(pub Date);

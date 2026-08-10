//! War entities: a declared state of hostility between two kingdoms over a
//! casus belli and a list of demands.
//!
//! A war is a separate entity kind — it links two kingdoms (attacker +
//! defender) through Bevy relationships, with the relationship hooks
//! maintaining the reverses on the kingdom
//! ([`KingdomHasWarsAttacking`], [`KingdomHasWarsDefending`]). The
//! casus belli is just a tag on the war ([`WarCasusBelliType`]); the
//! concrete things the war is fought over sit in [`WarDemands`].
//!
//! Spawned by [`crate::commands::declare_war::DeclareWar`] on the player's
//! pick of defender kingdom and CB type. The war has no status / no tick
//! yet — those land later. Today the entity is just the link graph plus
//! its demands list; the chronicle line in `DeclareWar` is the only
//! observable. The demands are resolved through
//! [`crate::commands::enforce_demands::EnforceDemands`].
//!
//! Pairing with [`super::kingdom`]: the reverse targets
//! (`KingdomHasWarsAttacking`, `KingdomHasWarsDefending`) live there
//! because that is where they sit — the relationship-colocation rule.

use super::kingdom::{KingdomHasWarsAttacking, KingdomHasWarsDefending};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A declared war. The two belligerents are in [`WarAttackerKingdom`] and
/// [`WarDefenderKingdom`]; the *why* is in [`WarCasusBelliType`]; the
/// concrete demands are in [`WarDemands`].
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

/// The casus belli backing this war — the *shape* of the fight. `Conquest`
/// is the only variant today; the variant name serializes. The concrete
/// demands the war makes on the defender sit in [`WarDemands`] — for a
/// `Conquest` war, [`DeclareWar`] seeds the list with one
/// [`WarDemandType::Take`] on the defender kingdom, but the two fields are
/// independent so a future CB shape can carry a different demand mix.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WarCasusBelliType {
    #[default]
    Conquest = 1,
}

/// The shape of a single war demand — what the attacker wants the war to
/// achieve on the demand's target kingdom. `Take` = "conquer the target
/// kingdom and absorb it into the attacker's realm" (resolved by
/// [`crate::commands::enforce_demands::EnforceDemands`] when the
/// target's land is held by one of the attacker's armies). New variants
/// are additive on this enum + a match arm in the enforce command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarDemandType {
    Take = 1,
}

/// One concrete demand a war is fought over — a `(shape, target_kingdom)`
/// pair. The data carrier; the war entity carries a `Vec<WarDemand>` in
/// [`WarDemands`].
#[derive(Debug, Clone, Copy)]
pub struct WarDemand {
    pub demand_type: WarDemandType,
    pub target: Entity,
}

/// The list of demands a war is fought over. Empty list is allowed (a war
/// with no demands just exists as a relationship graph entry; only `Take`
/// demands are enforceable today, so an empty list is effectively a war
/// that can never resolve). Sits on the war entity — the relationship
/// graph already lives on the war, no need for a separate CB entity.
#[derive(Component, Debug, Clone, Default)]
pub struct WarDemands(pub Vec<WarDemand>);

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

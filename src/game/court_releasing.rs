//! Release the court of a kingdom that has just been taken over via a
//! `Take` demand — despawn every non-Ruler courtier entity serving the
//! target kingdom.
//!
//! When the player enforces `Take` on a war, the target kingdom's
//! Ruler flips to the player (see
//! [`enforce_take`](crate::commands::enforce_demands::enforce_take));
//! this observer evicts every *other* courtier serving that kingdom so
//! the new regime starts with a clean court (the previous ruler's
//! people are "released" rather than carrying over).
//!
//! Bevy's relationship hook on
//! [`CourtierOfKingdom`](crate::ecs::CourtierOfKingdom) prunes the
//! entries from the kingdom's
//! [`KingdomHasCourtiers`](crate::ecs::KingdomHasCourtiers) as each
//! courtier despawns; the same hook on
//! [`CourtierOfCharacter`](crate::ecs::CourtierOfCharacter) prunes
//! them from each served character's
//! [`CharacterHasCourtiers`](crate::ecs::CharacterHasCourtiers).
//!
//! Runs as a Bevy observer for
//! [`OnDemandEnforced`](crate::observers::OnDemandEnforced). Only `Take`
//! triggers a release; new variants on
//! [`WarDemandType`](crate::ecs::WarDemandType) are additive and can
//! opt in here.
//!
//! ponytail: one observer, two passes — snapshot `(entity, string_id)`
//! pairs so we can deregister, then despawn + remove from `Registry`.
//! Courtier count per kingdom is small (a handful at most).
use crate::ecs::{CourtierType, KingdomHasCourtiers, Registry, StringId, WarDemandType};
use crate::observers::OnDemandEnforced;
use bevy::prelude::*;

/// Observer for [`OnDemandEnforced`] on
/// [`Take`](crate::ecs::WarDemandType::Take). Despawns every
/// courtier entity serving the target kingdom — except the new
/// Ruler, which `enforce_take` swaps before firing this trigger so
/// the freshly-spawned one survives the sweep. Removes ids from the
/// [`Registry`]. Bevy's relationship hooks prune the courtier out of
/// the kingdom's `KingdomHasCourtiers` and the served character's
/// `CharacterHasCourtiers` as part of the despawn.
pub fn on_demand_enforced(
    trigger: On<OnDemandEnforced>,
    kingdom_has_courtiers: Query<&KingdomHasCourtiers>,
    string_ids: Query<&StringId>,
    courtier_types: Query<&CourtierType>,
    mut commands: Commands,
    mut registry: ResMut<Registry>,
) {
    let event = trigger.event();
    if !matches!(event.demand_type, WarDemandType::Take) {
        return;
    }
    let target_kingdom = event.target;

    // Snapshot (entity, string id) pairs up front — despawning while
    // iterating `KingdomHasCourtiers` would mutate the same Vec, and
    // we need the id to deregister. Skip `type: Ruler` so the new
    // ruler that `enforce_take` just spawned survives.
    let Ok(kingdom_has_courtiers) = kingdom_has_courtiers.get(target_kingdom) else {
        return;
    };
    let to_release: Vec<(Entity, String)> = kingdom_has_courtiers
        .iter()
        .filter(|c: &Entity| !matches!(courtier_types.get(*c), Ok(CourtierType::Ruler)))
        .filter_map(|e| string_ids.get(e).ok().map(|s| (e, s.0.clone())))
        .collect();

    for (courtier_e, id) in to_release {
        if let Ok(mut ec) = commands.get_entity(courtier_e) {
            ec.despawn();
        }
        registry.by_id.remove(&id);
    }
}

//! Lookups for "who rules what" derived from Ruler courtiers.
//!
//! Before this module, a Bevy relationship (KingdomLedBy / CharacterLeads)
//! cached the leader link. With the leader defined by a `Courtier` of
//! `type: Ruler`, both relationship components were dropped and the
//! lookups below replace them — `KingdomHasCourtiers` and
//! `CharacterHasCourtiers` (auto-maintained by Bevy from `CourtierOfKingdom`
//! and `CourtierOfCharacter`) do the bookkeeping for free.
//!
//! Both helpers walk the target's courtier collection, which is short
//! (a handful at most per kingdom or character) — O(K) where K is the
//! courtier count for the queried entity.

use crate::ecs::{
    CharacterHasCourtiers, Courtier, CourtierOfCharacter, CourtierOfKingdom, CourtierType,
    KingdomHasCourtiers, Registry, StringId,
};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{Query, RelationshipTarget};

/// The character that rules `kingdom_e` — the courtier of `type: Ruler`
/// serving that kingdom. `None` when the kingdom is leaderless (no Ruler
/// courtier, or the kingdom is torn between rulers and none is `Ruler`).
pub fn get_kingdom_ruler(world: &World, kingdom_e: Entity) -> Option<Entity> {
    let khc = world.get::<KingdomHasCourtiers>(kingdom_e)?;
    khc.iter().find_map(|c| {
        if matches!(world.get::<CourtierType>(c), Some(CourtierType::Ruler)) {
            world.get::<CourtierOfCharacter>(c).map(|x| x.0)
        } else {
            None
        }
    })
}

/// The kingdoms `character_e` rules — every kingdom where `character_e` is
/// the Ruler courtier. A character can lead several kingdoms (conquest
/// transfer); the result is in `KingdomHasCourtiers` insertion order.
pub fn get_character_ruled_kingdoms(world: &World, character_e: Entity) -> Vec<Entity> {
    let Some(chc) = world.get::<CharacterHasCourtiers>(character_e) else {
        return Vec::new();
    };
    chc.iter()
        .filter(|c| matches!(world.get::<CourtierType>(*c), Some(CourtierType::Ruler)))
        .filter_map(|c| world.get::<CourtierOfKingdom>(c).map(|k| k.0))
        .collect()
}

/// Same lookup as [`get_character_ruled_kingdoms`] but driven by `Query` system
/// params. Used by Bevy systems that already hold other SystemParams —
/// `&World` cannot coexist with `Gizmos` / `ResMut<T>` / mut `Query` because
/// Bevy panics on conflicting access (`B0001`).
pub fn get_character_ruled_kingdoms_q(
    character_has_courtiers: &Query<&CharacterHasCourtiers>,
    courtier_types: &Query<&CourtierType>,
    courtier_of_kingdoms: &Query<&CourtierOfKingdom>,
    character_e: Entity,
) -> Vec<Entity> {
    let Ok(chc) = character_has_courtiers.get(character_e) else {
        return Vec::new();
    };
    chc.iter()
        .filter(|&c| courtier_types.get(c).ok() == Some(&CourtierType::Ruler))
        .filter_map(|c| courtier_of_kingdoms.get(c).ok().map(|k| k.0))
        .collect()
}

/// Same lookup as [`get_kingdom_ruler`] but driven by `Query` system params —
/// see [`get_character_ruled_kingdoms_q`] for why the `&World` variant doesn't
/// compose with other system params.
pub fn get_kingdom_ruler_q(
    kingdom_has_courtiers: &Query<&KingdomHasCourtiers>,
    courtier_types: &Query<&CourtierType>,
    courtier_of_characters: &Query<&CourtierOfCharacter>,
    kingdom_e: Entity,
) -> Option<Entity> {
    let Ok(khc) = kingdom_has_courtiers.get(kingdom_e) else {
        return None;
    };
    khc.iter().find_map(|c| {
        if courtier_types.get(c).ok() == Some(&CourtierType::Ruler) {
            courtier_of_characters.get(c).ok().map(|x| x.0)
        } else {
            None
        }
    })
}

/// Set or clear the Ruler courtier serving `kingdom_e`. Pass `None` to
/// despawn the current Ruler (kingdom is leaderless); pass `Some(e)` to
/// swap to a new leader. Bevy's relationship hooks keep
/// `KingdomHasCourtiers` and `CharacterHasCourtiers` in sync.
///
/// Used by succession (`src/game/inheriting.rs`) and
/// [`enforce_take`](crate::commands::enforce_demands::enforce_take) —
/// the two sites that change a kingdom's leader at runtime.
pub fn set_ruler(world: &mut World, kingdom_e: Entity, new_leader_e: Option<Entity>) {
    // Find and despawn any existing Ruler for this kingdom. Bevy's hook on
    // `CourtierOfKingdom` prunes the entry from `KingdomHasCourtiers` and
    // the served character's `CharacterHasCourtiers` automatically.
    let old: Option<Entity> = world
        .get::<KingdomHasCourtiers>(kingdom_e)
        .and_then(|khc| {
            khc.iter().find(|c| matches!(world.get::<CourtierType>(*c), Some(CourtierType::Ruler)))
        });
    if let Some(old) = old {
        let old_id = world.get::<StringId>(old).map(|s| s.0.clone());
        world.entity_mut(old).despawn();
        if let Some(old_id) = old_id {
            world.resource_mut::<Registry>().by_id.remove(&old_id);
        }
    }

    let Some(new_leader_e) = new_leader_e else {
        return;
    };

    // Spawn the new Ruler. The id encodes the leader so successive swaps on
    // the same kingdom don't collide; ids are cosmetic — the `Entity` is what
    // gameplay reads through Bevy relationship targets.
    let new_id = format!("courtier-ruler-{new_leader_e:?}");
    let eid = world
        .spawn((
            StringId(new_id.clone()),
            Courtier,
            CourtierType::Ruler,
            CourtierOfCharacter(new_leader_e),
            CourtierOfKingdom(kingdom_e),
        ))
        .id();
    world.resource_mut::<Registry>().insert(new_id, eid);
}
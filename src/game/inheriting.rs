//! Kingdom succession when a leader dies.
//!
//! An observer for [`OnCharacterDied`] — sole consumer for succession. The
//! death system is the only publisher; the inheriting code is the only one
//! that swaps a kingdom's Ruler courtier.
//!
//! The heir ladder (alive-only with fall-through):
//!   1. Eldest alive **son** — earliest `dob` among the dead's
//!      [`CharacterHasFatheredChildren`] that are also [`CharacterIsAlive`] +
//!      [`CharacterGender::Male`].
//!   2. Eldest alive **male sibling** — anyone sharing father or mother
//!      (deduped), alive + male, earliest `dob`.
//!   3. Eldest alive **male of the house** — earliest `dob` among all alive +
//!      male characters sharing [`CharacterOfHouse`] with the dead leader.
//!   4. None of the above → despawn the Ruler, leaving the kingdom
//!      leaderless. "Leaderless" is derivable (no Ruler courtier); no
//!      separate marker is kept.
//!
//! Each succession (with or without an heir) fires [`OnKingdomSucceeded`].
//!
//! Gold is a realm treasury, not a leader's purse. When the leader changes,
//! the kingdom's `KingdomGold` stays with the kingdom — the new leader
//! inherits the realm's existing treasury unchanged.
//!
//! If the dead character is the player's character, `Ctx::player_character_id`
//! is reassigned to the primary heir's `StringId`, or set to `None` if no
//! heir exists. The game keeps running either way; UI panels and commands
//! already short-circuit on a missing registry hit.
//!
//! All real work happens inside a queued `move |world: &mut World|` closure —
//! Bevy 0.19 forbids observers from taking `&World` alongside `Query<&mut T>`
//! (read-all + write-T conflicts), so we defer the body until after the
//! observer flushes where exclusive world access is fine. Same pattern as
//! [`presenting_event::on_event_resolved`](crate::game::presenting_event::on_event_resolved)
//! and [`yielding::on_building_updated`](crate::game::yielding::on_building_updated).

use crate::app::Game;
use crate::ecs::{
    Character, CharacterDateOfBirth, CharacterGender, CharacterHasFather,
    CharacterHasFatheredChildren, CharacterHasMother, CharacterIsAlive, CharacterOfHouse,
    Courtier, CourtierOfCharacter, CourtierOfKingdom, CourtierType, KingdomHasCourtiers,
    Registry, StringId,
};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
use crate::observers::{OnCharacterDied, OnKingdomSucceeded, SuccessionRelation};
use crate::resources::date::Date;
use bevy::prelude::*;

pub fn on_character_died(trigger: On<OnCharacterDied>, mut commands: Commands) {
    let dead = trigger.event().character;
    commands.queue(move |world: &mut World| {
        let kingdoms: Vec<Entity> = get_character_ruled_kingdoms(world, dead);

        // Build the per-character query states we need. They borrow world
        // mutably; calls below happen inside a `commands.queue` closure so
        // exclusive access is fine.
        let mut characters = world.query_filtered::<(
            Entity,
            &CharacterOfHouse,
            &CharacterGender,
            &CharacterIsAlive,
            &CharacterDateOfBirth,
        ), With<Character>>();
        let mut string_ids = world.query_filtered::<(Entity, &StringId), With<Character>>();
        let mut fathered = world.query::<&CharacterHasFatheredChildren>();
        let mut fathers = world.query::<&CharacterHasFather>();
        let mut mothers = world.query::<&CharacterHasMother>();
        let mut kingdom_has_courtiers = world.query::<&KingdomHasCourtiers>();
        let mut courtier_types = world.query::<&CourtierType>();

        // The "primary heir" — the heir of the first kingdom that resolves to one.
        let primary_heir: Option<Entity> = kingdoms
            .iter()
            .find_map(|&_k| pick_heir(dead, world, &mut characters, &mut fathered, &mut fathers, &mut mothers))
            .map(|(e, _)| e);

        for kingdom in kingdoms {
            let pick = pick_heir(dead, world, &mut characters, &mut fathered, &mut fathers, &mut mothers);
            match pick {
                Some((new_leader, relation)) => {
                    set_ruler(
                        world,
                        &mut kingdom_has_courtiers,
                        &mut string_ids,
                        &mut courtier_types,
                        kingdom,
                        Some(new_leader),
                    );
                    world.trigger(OnKingdomSucceeded {
                        kingdom,
                        from: dead,
                        to: Some(new_leader),
                        relation,
                    });
                }
                None => {
                    set_ruler(
                        world,
                        &mut kingdom_has_courtiers,
                        &mut string_ids,
                        &mut courtier_types,
                        kingdom,
                        None,
                    );
                    world.trigger(OnKingdomSucceeded {
                        kingdom,
                        from: dead,
                        to: None,
                        relation: SuccessionRelation::Leaderless,
                    });
                }
            }
        }

        // Player swap: if the dead was the player, hand the seat to the
        // primary heir. Snapshot reads first to drop the `Res<Game>` borrow
        // before re-borrowing mutably to write.
        let (was_player, new_player_id) = {
            let game = world.resource::<Game>();
            let dead_string_id = string_ids
                .iter(world)
                .find_map(|(e, sid)| (e == dead).then(|| sid.0.clone()));
            let was_player = matches!(
                (&game.ctx.player_character_id, dead_string_id),
                (Some(player_id), Some(dead_id)) if player_id == &dead_id
            );
            let new_player_id = primary_heir.and_then(|h| {
                string_ids
                    .get(world, h)
                    .ok()
                    .map(|(_, sid)| sid.0.clone())
            });
            (was_player, new_player_id)
        };
        if was_player {
            let mut game = world.resource_mut::<Game>();
            game.ctx.player_character_id = new_player_id;
        }
    });
}

/// Replace the Ruler courtier serving `kingdom_e`. Pass `None` to clear
/// (kingdom becomes leaderless); pass `Some(e)` to swap to a new leader.
/// Runs inside a queued closure with `&mut World` access — despawn and
/// spawn happen immediately, Bevy's relationship hooks update
/// `KingdomHasCourtiers` / `CharacterHasCourtiers` automatically.
/// Registry access is inlined (grab `world.resource_mut::<Registry>()`
/// at the moment of the write) so the function's borrow of `world` doesn't
/// conflict with the caller holding other query states.
fn set_ruler(
    world: &mut World,
    kingdom_has_courtiers: &mut QueryState<&KingdomHasCourtiers>,
    string_ids: &mut QueryState<(Entity, &StringId), With<Character>>,
    courtier_types: &mut QueryState<&CourtierType>,
    kingdom_e: Entity,
    new_leader_e: Option<Entity>,
) {
    // Despawn the existing Ruler (if any). Bevy auto-prunes the
    // `KingdomHasCourtiers` / `CharacterHasCourtiers` entries on despawn.
    if let Ok(khc) = kingdom_has_courtiers.get(world, kingdom_e) {
        let old: Option<Entity> = khc
            .iter()
            .find(|c: &Entity| courtier_types.get(world, *c).ok() == Some(&CourtierType::Ruler));
        if let Some(old) = old {
            let old_id = string_ids.get(world, old).ok().map(|(_, s)| s.0.clone());
            world.entity_mut(old).despawn();
            if let Some(old_id) = old_id {
                world.resource_mut::<Registry>().by_id.remove(&old_id);
            }
        }
    }
    // Spawn the new Ruler.
    let Some(new_leader_e) = new_leader_e else {
        return;
    };
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

/// Resolves the heir per the four-tier ladder. Returns `None` only when no
/// alive male candidate exists in the dead character's lineage or house.
/// Takes `QueryState`s — created inside the queued closure where `&mut World`
/// is available.
#[allow(clippy::too_many_arguments)]
fn pick_heir(
    dead: Entity,
    world: &World,
    characters: &mut QueryState<
        (
            Entity,
            &CharacterOfHouse,
            &CharacterGender,
            &CharacterIsAlive,
            &CharacterDateOfBirth,
        ),
        With<Character>,
    >,
    fathered: &mut QueryState<&CharacterHasFatheredChildren>,
    fathers: &mut QueryState<&CharacterHasFather>,
    mothers: &mut QueryState<&CharacterHasMother>,
) -> Option<(Entity, SuccessionRelation)> {
    // Filter an entity to `alive + male`, optionally constrained to `house`.
    let mut alive_male_in = |candidate: Entity, house: Option<Entity>| -> Option<(Entity, Date)> {
        if candidate == dead {
            return None;
        }
        let (_, coh, gender, alive, dob) = characters.get(world, candidate).ok()?;
        if !alive.0 || !matches!(gender, CharacterGender::Male) {
            return None;
        }
        if let Some(h) = house {
            if coh.0 != h {
                return None;
            }
        }
        Some((candidate, dob.0))
    };

    // 1. Eldest alive son — via the dead character's CharacterHasFatheredChildren.
    if let Ok(children) = fathered.get(world, dead) {
        let mut cands: Vec<_> = children
            .children()
            .iter()
            .filter_map(|&c| alive_male_in(c, None))
            .collect();
        if !cands.is_empty() {
            cands.sort_by_key(|(_, d)| *d);
            return Some((cands[0].0, SuccessionRelation::EldestSon));
        }
    }

    // 2. Eldest alive male sibling — anyone sharing father or mother (deduped).
    let father = fathers.get(world, dead).ok().map(|c| c.0);
    let mother = mothers.get(world, dead).ok().map(|c| c.0);
    let mut siblings: Vec<Entity> = Vec::new();
    if let Some(f) = father {
        if let Ok(children) = fathered.get(world, f) {
            for &s in children.children() {
                if s != dead && !siblings.contains(&s) {
                    siblings.push(s);
                }
            }
        }
    }
    if let Some(m) = mother {
        if let Ok(children) = fathered.get(world, m) {
            for &s in children.children() {
                if s != dead && !siblings.contains(&s) {
                    siblings.push(s);
                }
            }
        }
    }
    let mut cands: Vec<_> = siblings
        .into_iter()
        .filter_map(|c| alive_male_in(c, None))
        .collect();
    if !cands.is_empty() {
        cands.sort_by_key(|(_, d)| *d);
        return Some((cands[0].0, SuccessionRelation::MaleSibling));
    }

    // 3. Eldest alive male of the dead character's house.
    if let Ok((_, coh, _, _, _)) = characters.get(world, dead) {
        let dead_house = coh.0;
        let mut cands: Vec<_> = characters
            .iter(world)
            .filter_map(|(e, coh2, gender, alive, dob)| {
                if e == dead || coh2.0 != dead_house {
                    return None;
                }
                if !alive.0 || !matches!(gender, CharacterGender::Male) {
                    return None;
                }
                Some((e, dob.0))
            })
            .collect();
        if !cands.is_empty() {
            cands.sort_by_key(|(_, d)| *d);
            return Some((cands[0].0, SuccessionRelation::ElderOfHouse));
        }
    }

    None
}
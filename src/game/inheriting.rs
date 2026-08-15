//! Kingdom succession when a leader dies.
//!
//! An observer for [`OnCharacterDied`] — sole consumer for succession. The
//! death system is the only publisher; the inheriting code is the only one
//! that reassigns [`KingdomLedBy`].
//!
//! The heir ladder (alive-only with fall-through):
//!   1. Eldest alive **son** — earliest `dob` among the dead's
//!      [`CharacterHasFatheredChildren`] that are also [`CharacterIsAlive`] +
//!      [`CharacterGender::Male`].
//!   2. Eldest alive **male sibling** — anyone sharing father or mother
//!      (deduped), alive + male, earliest `dob`.
//!   3. Eldest alive **male of the house** — earliest `dob` among all alive +
//!      male characters sharing [`CharacterOfHouse`] with the dead leader.
//!   4. None of the above → mark the kingdom with [`KingdomLeaderless`] and
//!      remove [`KingdomLedBy`].
//!
//! Each succession (with or without an heir) fires [`OnKingdomSucceeded`].
//!
//! When the dead leader has a heir, their `CharacterGold` transfers to the
//! heir of their first successor kingdom (the "primary heir" — i.e. the first
//! kingdom in `CharacterLeads::kingdoms()` order that resolves to a heir).
//! If no kingdom yields an heir, the gold is cleared from the dead character
//! and otherwise evaporates. Multi-kingdom leaders where every kingdom goes
//! to a different heir still funnel the dead's treasury into the primary
//! heir; the others inherit empty pots.
//!
//! If the dead character is the player's character, `Ctx::player_character_id`
//! is reassigned to the primary heir's `StringId`, or set to `None` if no
//! heir exists. The game keeps running either way; UI panels and commands
//! already short-circuit on a missing registry hit.

use crate::app::Game;
use crate::ecs::{
    Character, CharacterDateOfBirth, CharacterGender, CharacterGold, CharacterHasFather,
    CharacterHasFatheredChildren, CharacterHasMother, CharacterIsAlive, CharacterLeads,
    CharacterOfHouse, KingdomLedBy, KingdomLeaderless, StringId,
};
use crate::observers::{OnCharacterDied, OnKingdomSucceeded, SuccessionRelation};
use crate::resources::date::Date;
use bevy::prelude::*;

pub fn on_character_died(
    trigger: On<OnCharacterDied>,
    mut commands: Commands,
    mut game: ResMut<Game>,
    character_leads: Query<&CharacterLeads, With<Character>>,
    characters: Query<
        (
            Entity,
            &CharacterOfHouse,
            &CharacterGender,
            &CharacterIsAlive,
            &CharacterDateOfBirth,
        ),
        With<Character>,
    >,
    mut character_golds: Query<&mut CharacterGold, With<Character>>,
    string_ids: Query<(Entity, &StringId), With<Character>>,
    fathered: Query<&CharacterHasFatheredChildren>,
    fathers: Query<&CharacterHasFather>,
    mothers: Query<&CharacterHasMother>,
) {
    let dead = trigger.event().character;

    let kingdoms: Vec<Entity> = character_leads
        .get(dead)
        .map(|cl| cl.kingdoms().to_vec())
        .unwrap_or_default();

    // The "primary heir" — the heir of the first kingdom that resolves to one.
    // Used for gold transfer and (if the dead was the player) player swap.
    let primary_heir: Option<Entity> = kingdoms
        .iter()
        .find_map(|&_k| pick_heir(dead, &characters, &fathered, &fathers, &mothers))
        .map(|(e, _)| e);

    let mut gold_settled = false;
    for kingdom in kingdoms {
        let pick = pick_heir(dead, &characters, &fathered, &fathers, &mothers);
        match pick {
            Some((new_leader, relation)) => {
                // Gold transfer: once, to the primary heir only.
                if Some(new_leader) == primary_heir && !gold_settled {
                    if let Ok(dead_gold) = character_golds.get(dead) {
                        let dead_amount = dead_gold.0;
                        if let Ok(mut heir_gold) = character_golds.get_mut(new_leader) {
                            heir_gold.0 += dead_amount;
                        }
                    }
                    if let Ok(mut dead_gold) = character_golds.get_mut(dead) {
                        dead_gold.0 = 0;
                    }
                    gold_settled = true;
                }
                commands.entity(kingdom).insert(KingdomLedBy(new_leader));
                commands.trigger(OnKingdomSucceeded {
                    kingdom,
                    from: dead,
                    to: Some(new_leader),
                    relation,
                });
            }
            None => {
                commands
                    .entity(kingdom)
                    .remove::<KingdomLedBy>()
                    .insert(KingdomLeaderless);
                commands.trigger(OnKingdomSucceeded {
                    kingdom,
                    from: dead,
                    to: None,
                    relation: SuccessionRelation::Leaderless,
                });
            }
        }
    }

    // All kingdoms went leaderless — clear the dead's gold for hygiene.
    if primary_heir.is_none() {
        if let Ok(mut dead_gold) = character_golds.get_mut(dead) {
            dead_gold.0 = 0;
        }
    }

    // Player swap: if the dead was the player, hand the seat to the primary
    // heir, or vacate it if none exists.
    let dead_string_id = string_ids
        .iter()
        .find_map(|(e, string_id)| (e == dead).then(|| string_id.0.clone()));
    let was_player = matches!(
        (&game.ctx.player_character_id, dead_string_id),
        (Some(player_id), Some(dead_id)) if player_id == &dead_id
    );
    if was_player {
        game.ctx.player_character_id = primary_heir.and_then(|h| {
            string_ids
                .get(h)
                .ok()
                .map(|(_, string_id)| string_id.0.clone())
        });
    }
}

/// Resolves the heir per the four-tier ladder. Returns `None` only when no
/// alive male candidate exists in the dead character's lineage or house.
fn pick_heir(
    dead: Entity,
    characters: &Query<
        (
            Entity,
            &CharacterOfHouse,
            &CharacterGender,
            &CharacterIsAlive,
            &CharacterDateOfBirth,
        ),
        With<Character>,
    >,
    fathered: &Query<&CharacterHasFatheredChildren>,
    fathers: &Query<&CharacterHasFather>,
    mothers: &Query<&CharacterHasMother>,
) -> Option<(Entity, SuccessionRelation)> {
    // Filter an entity to `alive + male`, optionally constrained to `house`.
    let alive_male_in = |candidate: Entity, house: Option<Entity>| -> Option<(Entity, Date)> {
        if candidate == dead {
            return None;
        }
        let (_, coh, gender, alive, dob) = characters.get(candidate).ok()?;
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
    if let Ok(children) = fathered.get(dead) {
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
    let father = fathers.get(dead).ok().map(|c| c.0);
    let mother = mothers.get(dead).ok().map(|c| c.0);
    let mut siblings: Vec<Entity> = Vec::new();
    if let Some(f) = father {
        if let Ok(children) = fathered.get(f) {
            for &s in children.children() {
                if s != dead && !siblings.contains(&s) {
                    siblings.push(s);
                }
            }
        }
    }
    if let Some(m) = mother {
        if let Ok(children) = fathered.get(m) {
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
    if let Ok((_, coh, _, _, _)) = characters.get(dead) {
        let dead_house = coh.0;
        let mut cands: Vec<_> = characters
            .iter()
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

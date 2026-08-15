//! Kingdom succession when a leader dies.
//!
//! An observer for [`OnCharacterDied`] — sole consumer for succession. The
//! death system is the only publisher; the inheriting code is the only one
//! that reassigns [`KingdomLedBy`].
//!
//! The heir ladder (alive-only with fall-through):
//!   1. Eldest alive **son** — earliest `dob` among the dead's
//!      [`CharacterHasFatheredChildren`] that are also [`CharacterIsAlive`] +
//!      [`CharacterSex::Male`].
//!   2. Eldest alive **male sibling** — anyone sharing father or mother
//!      (deduped), alive + male, earliest `dob`.
//!   3. Eldest alive **male of the house** — earliest `dob` among all alive +
//!      male characters sharing [`CharacterOfHouse`] with the dead leader.
//!   4. None of the above → mark the kingdom with [`KingdomLeaderless`] and
//!      remove [`KingdomLedBy`].
//!
//! Each succession (with or without an heir) fires [`OnKingdomSucceeded`].

use crate::ecs::{
    Character, CharacterDateOfBirth, CharacterHasFather, CharacterHasFatheredChildren,
    CharacterHasMother, CharacterIsAlive, CharacterLeads, CharacterOfHouse, CharacterSex,
    KingdomLedBy, KingdomLeaderless,
};
use crate::events::{OnCharacterDied, OnKingdomSucceeded, SuccessionRelation};
use crate::resources::date::Date;
use bevy::prelude::*;

pub fn on_character_died(
    trigger: On<OnCharacterDied>,
    mut commands: Commands,
    character_leads: Query<&CharacterLeads, With<Character>>,
    characters: Query<
        (
            Entity,
            &CharacterOfHouse,
            &CharacterSex,
            &CharacterIsAlive,
            &CharacterDateOfBirth,
        ),
        With<Character>,
    >,
    fathered: Query<&CharacterHasFatheredChildren>,
    fathers: Query<&CharacterHasFather>,
    mothers: Query<&CharacterHasMother>,
) {
    let dead = trigger.event().character;

    let kingdoms: Vec<Entity> = character_leads
        .get(dead)
        .map(|cl| cl.kingdoms().to_vec())
        .unwrap_or_default();

    for kingdom in kingdoms {
        let pick = pick_heir(dead, &characters, &fathered, &fathers, &mothers);
        match pick {
            Some((new_leader, relation)) => {
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
}

/// Resolves the heir per the four-tier ladder. Returns `None` only when no
/// alive male candidate exists in the dead character's lineage or house.
fn pick_heir(
    dead: Entity,
    characters: &Query<
        (
            Entity,
            &CharacterOfHouse,
            &CharacterSex,
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
        let (_, coh, sex, alive, dob) = characters.get(candidate).ok()?;
        if !alive.0 || !matches!(sex, CharacterSex::Male) {
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
            .filter_map(|(e, coh2, sex, alive, dob)| {
                if e == dead || coh2.0 != dead_house {
                    return None;
                }
                if !alive.0 || !matches!(sex, CharacterSex::Male) {
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

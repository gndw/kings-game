//! The monthly payout: every ruler's accumulated yield paid into their
//! treasury.

use crate::ecs::{Character, CharacterGold, CharacterGoldYield};
use crate::helper::kingdom_helper::character_ruled_kingdoms;
use bevy::prelude::*;

/// Pay every character that leads a kingdom their monthly gold yield. Only
/// characters who are the Ruler courtier of at least one kingdom earn; a
/// leader whose yield is zero pays nothing (`gold += 0`), and a negative
/// yield deepens debt with no floor. Runs in the [`crate::schedules::OnMonth`]
/// schedule, fired on month rollover.
///
/// Two passes — first collect the leader entities (immutable borrow of world),
/// then pay them (mutable borrow). The split dodges a borrow conflict between
/// `Query::iter_mut` and `character_ruled_kingdoms`.
pub fn on_month(world: &mut World) {
    // Pass 1: collect every leader's entity. Uses two immutable borrows —
    // the character set and the courtier scan — neither mutates.
    let leaders: Vec<Entity> = {
        let mut characters = world.query_filtered::<Entity, With<Character>>();
        let mut out: Vec<Entity> = Vec::new();
        for char_e in characters.iter(world) {
            if character_ruled_kingdoms(world, char_e).is_empty() {
                continue;
            }
            out.push(char_e);
        }
        out
    };

    // Pass 2: pay the leaders. The borrow on `world` from pass 1 has ended.
    let mut query = world.query::<(&mut CharacterGold, &CharacterGoldYield)>();
    for char_e in leaders {
        let Ok((mut character_gold, character_gold_yield)) = query.get_mut(world, char_e) else {
            continue;
        };
        character_gold.0 += character_gold_yield.0;
    }
}

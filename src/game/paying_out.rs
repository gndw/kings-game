//! The monthly payout: every kingdom's accumulated yield paid into its
//! treasury. Gold is a realm treasury, not a leader's purse — the leader
//! is the steward, the kingdom is the owner.

use crate::ecs::{KingdomGold, KingdomGoldYield};
use bevy::prelude::*;

/// Pay every kingdom its monthly gold yield. Runs in the
/// [`crate::schedules::OnMonth`] schedule, fired on month rollover.
///
/// A kingdom with `gold_yield == 0` pays nothing (`gold += 0`); a negative
/// yield deepens debt with no floor.
pub fn on_month(world: &mut World) {
    let mut kingdoms = world.query::<(&mut KingdomGold, &KingdomGoldYield)>();
    for (mut gold, gy) in kingdoms.iter_mut(world) {
        gold.0 += gy.0;
    }
}

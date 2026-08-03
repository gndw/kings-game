//! The monthly payout: every ruler's accumulated yield paid into their
//! treasury.

use crate::ecs::{CharacterGold, CharacterGoldYield, Leads};
use bevy::prelude::*;

/// Pay every character that leads a kingdom their monthly gold yield. Only
/// leaders (those carrying [`Leads`], the reverse of [`crate::ecs::LedBy`])
/// earn; a leader whose yield is zero pays nothing (`gold += 0`), and a
/// negative yield deepens debt with no floor. Runs in the
/// [`crate::schedules::OnMonth`] schedule, fired on month rollover.
pub fn payout(mut leaders: Query<(&mut CharacterGold, &CharacterGoldYield), With<Leads>>) {
    for (mut gold, yield_per_mo) in &mut leaders {
        gold.0 += yield_per_mo.0;
    }
}

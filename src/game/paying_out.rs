//! The monthly payout: every ruler's accumulated yield paid into their
//! treasury.

use crate::ecs::{CharacterGold, CharacterGoldYield, CharacterLeads};
use bevy::prelude::*;

/// Pay every character that leads a kingdom their monthly gold yield. Only
/// leaders (those carrying [`CharacterLeads`], the reverse of
/// [`crate::ecs::KingdomLedBy`]) earn; a leader whose yield is zero pays
/// nothing (`gold += 0`), and a negative yield deepens debt with no floor.
/// Runs in the [`crate::schedules::OnMonth`] schedule, fired on month rollover.
pub fn on_month(
    mut character_leads: Query<
        (&mut CharacterGold, &CharacterGoldYield),
        With<CharacterLeads>,
    >,
) {
    for (mut character_gold, character_gold_yield) in &mut character_leads {
        character_gold.0 += character_gold_yield.0;
    }
}

//! The monthly payout: every ruler's accumulated yield paid into their
//! treasury.

use crate::ecs::{CharacterState, Leads};
use bevy::prelude::*;

/// Pay every character that leads a kingdom their monthly gold yield. Only
/// leaders (those carrying [`Leads`], the reverse of [`crate::ecs::LedBy`])
/// earn; a leader whose yield is zero pays nothing (`gold += 0`), and a
/// negative yield deepens debt with no floor. Runs in the
/// [`crate::schedules::OnMonth`] schedule, fired on month rollover.
pub fn payout(mut leaders: Query<&mut CharacterState, With<Leads>>) {
    for mut cs in &mut leaders {
        cs.gold += cs.gold_yield;
    }
}

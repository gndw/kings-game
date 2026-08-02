//! The monthly tax payout: every ruler's accumulated yield paid into their
//! treasury, with the player's own finances chronicled.

use crate::app::Game;
use crate::ctx::Ctx;
use crate::resources::date::Date;
use crate::ecs::{CharacterState, EntityIndex, Registry};
use bevy::prelude::*;

/// Pay every ruler their monthly gold yield on the first of the month, and
/// chronicle the player's own finances. Scheduled in `FixedUpdate`, chained
/// after [`crate::updates::yields::recompute_yields`] (which sets the yield) and
/// the date-advancing [`crate::updates::tick::tick`], so it pays out the freshly-recomputed
/// yield for the new month. No-ops on every other day.
pub fn monthly_payout(mut game: ResMut<Game>, date: Res<Date>) {
    if !date.is_month_start() {
        return;
    }
    payout(&mut game.ctx);
}

/// Pay every ruler their monthly gold yield and chronicle the player's own
/// finances. A yield of zero pays nothing and says nothing; a negative yield
/// deepens debt with no floor — a ruler who keeps an army they can't afford
/// stays in the red until they lose the buildings or take someone else's
/// land. Only the player's line is worth a chronicle entry; everyone else's
/// would drown it.
pub fn payout(ctx: &mut Ctx) {
    let characters = ctx.world.resource::<EntityIndex>().characters.clone();
    let player_e = ctx
        .world
        .resource::<Registry>()
        .get(&ctx.player_character_id);
    for &ce in &characters {
        let income = match ctx.world.get::<CharacterState>(ce) {
            Some(cs) => cs.gold_yield,
            None => continue,
        };
        if income == 0 {
            continue;
        }
        if let Some(mut cs) = ctx.world.get_mut::<CharacterState>(ce) {
            cs.gold += income;
        }
        if Some(ce) == player_e {
            if income > 0 {
                ctx.chronicles
                    .push(format!("The realm renders {income} gold in taxes."));
            } else {
                ctx.chronicles
                    .push(format!("The realm's upkeep runs {} gold short.", -income));
            }
        }
    }
}

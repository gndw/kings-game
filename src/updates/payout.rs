//! The monthly tax payout: every ruler's accumulated yield paid into their
//! treasury, with the player's own finances chronicled.

use crate::app::Game;
use crate::ctx::Ctx;
use crate::ecs::{CharacterState, Registry};
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

/// Pay every ruler their monthly gold yield on the first of the month, and
/// chronicle the player's own finances. Scheduled in `FixedUpdate`, chained
/// after [`crate::updates::yields::recompute_yields`] (which sets the yield) and
/// the date-advancing [`crate::updates::tick::tick`], so it pays out the freshly-recomputed
/// yield for the new month. No-ops on every other day. Exclusive: it touches
/// both `CharacterState` and the `Game` resource's chronicle.
pub fn monthly_payout(world: &mut World) {
    if !world.resource::<Date>().is_month_start() {
        return;
    }
    world.resource_scope::<Game, _>(|world, mut game| {
        payout(world, &mut game.ctx);
    });
}

/// Pay every ruler their monthly gold yield and chronicle the player's own
/// finances. A yield of zero pays nothing and says nothing; a negative yield
/// deepens debt with no floor — a ruler who keeps an army they can't afford
/// stays in the red until they lose the buildings or take someone else's
/// land. Only the player's line is worth a chronicle entry; everyone else's
/// would drown it.
pub fn payout(world: &mut World, ctx: &mut Ctx) {
    let player_e = world.resource::<Registry>().get(&ctx.player_character_id);
    // Collect who earns what in a shared pass, then mutate — a `get_mut` can't
    // share the world with the `Registry`/`CharacterState` reads.
    let mut ops: Vec<(Entity, i64)> = Vec::new();
    let mut player_income: Option<i64> = None;
    for entity in world.iter_entities() {
        let Some(cs) = entity.get::<CharacterState>() else {
            continue;
        };
        if cs.gold_yield == 0 {
            continue;
        }
        ops.push((entity.id(), cs.gold_yield));
        if Some(entity.id()) == player_e {
            player_income = Some(cs.gold_yield);
        }
    }
    for (e, income) in ops {
        if let Some(mut cs) = world.get_mut::<CharacterState>(e) {
            cs.gold += income;
        }
    }
    if let Some(income) = player_income {
        if income > 0 {
            ctx.chronicles
                .push(format!("The realm renders {income} gold in taxes."));
        } else {
            ctx.chronicles
                .push(format!("The realm's upkeep runs {} gold short.", -income));
        }
    }
}

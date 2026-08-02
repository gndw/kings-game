//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::app::Game;
use crate::ecs::{Building, Built, CharacterState, EntityIndex, Holds, LedBy};
use bevy::ecs::world::World;
use bevy::prelude::*;
use std::collections::HashMap;

/// Recompute every character's `gold_yield` and `levy` from their holdings.
/// Scheduled in `FixedUpdate`, chained before [`crate::updates::tick::tick`] and
/// [`crate::updates::payout::monthly_payout`] (which pays out the freshly-
/// recomputed yield on month start). A first recompute runs at the end of
/// [`Ctx::new_game`](crate::ctx::Ctx::new_game), so the opening screen already
/// shows what a realm renders.
pub fn recompute_yields(mut game: ResMut<Game>) {
    recompute(&mut game.ctx.world);
}

/// Every ruler's realm summed, as `leader → (gold yield, levy)`. Gold yield is
/// profit less upkeep across the holdings; levy is the troop total. A character
/// who leads nothing is simply absent here, so [`recompute`] defaults them to
/// zero. `&World`-safe, like every read in the sim.
fn realm_totals(world: &World) -> HashMap<Entity, (i64, u64)> {
    let kingdoms = world.resource::<EntityIndex>().kingdoms.clone();
    let mut totals: HashMap<Entity, (i64, u64)> = HashMap::new();
    for &ke in &kingdoms {
        let leader = world.get::<LedBy>(ke).map(|l| l.0);
        let holds = world.get::<Holds>(ke);
        let (Some(leader), Some(holds)) = (leader, holds) else {
            continue;
        };
        let entry = totals.entry(leader).or_insert((0, 0));
        for &le in holds.0.iter() {
            let Some(built) = world.get::<Built>(le) else {
                continue;
            };
            for &be in built.0.iter() {
                let Some(b) = world.get::<Building>(be) else {
                    continue;
                };
                entry.0 += b.gold_profit as i64 - b.gold_upkeep as i64;
                entry.1 += b.levy as u64;
            }
        }
    }
    totals
}

/// Recompute every character's `gold_yield` and `levy` from their holdings, so
/// a gained or lost building shows up the next day. The shared body of the
/// [`recompute_yields`] system and the
/// [`Ctx::new_game`](crate::ctx::Ctx::new_game) seed.
pub fn recompute(world: &mut World) {
    let totals = realm_totals(world);
    let characters = world.resource::<EntityIndex>().characters.clone();
    for &ce in &characters {
        let (gold_yield, levy) = totals.get(&ce).copied().unwrap_or((0, 0));
        if let Some(mut cs) = world.get_mut::<CharacterState>(ce) {
            cs.gold_yield = gold_yield;
            cs.levy = levy;
        }
    }
}

//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::ecs::{Built, CharacterState, Holds, KingdomLedBy, LedBy};
use crate::resources::buildings::Buildings;
use bevy::ecs::world::World;
use bevy::prelude::*;
use std::collections::HashMap;

/// Recompute every character's `gold_yield` and `levy` from their holdings.
/// Scheduled in `FixedUpdate`, chained before [`crate::updates::tick::tick`] and
/// [`crate::updates::payout::monthly_payout`] (which pays out the freshly-
/// recomputed yield on month start). A first recompute runs in `main` after
/// [`crate::ecs::populate`], so the opening screen already shows what a realm
/// renders. Exclusive: it mixes component mutation with the `Buildings`
/// resource read.
pub fn recompute_yields(world: &mut World) {
    recompute(world);
}

/// Every ruler's realm summed, as `leader → (gold yield, levy)`. Gold yield is
/// profit less upkeep across the holdings; levy is the troop total. A character
/// who leads nothing is simply absent here, so [`recompute`] defaults them to
/// zero. `&World`-safe: `iter_entities` + `get` avoid the `&mut World` a
/// `Query` would need, and sidestep the resource-vs-world borrow that an
/// exclusive `query_mut` would create.
fn realm_totals(world: &World) -> HashMap<Entity, (i64, u64)> {
    let buildings = world.resource::<Buildings>();
    let mut totals: HashMap<Entity, (i64, u64)> = HashMap::new();
    for entity in world.iter_entities() {
        let (Some(leader), Some(holds)) = (entity.get::<LedBy>(), entity.get::<Holds>()) else {
            continue;
        };
        let entry = totals.entry(leader.0).or_insert((0, 0));
        for &le in holds.0.iter() {
            let Some(built) = world.get::<Built>(le) else {
                continue;
            };
            for bid in built.0.iter() {
                let Some(b) = buildings.get(bid) else {
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
/// [`recompute_yields`] system and the `main` seed.
pub fn recompute(world: &mut World) {
    let totals = realm_totals(world);
    let mut q = world.query::<(Entity, &mut CharacterState)>();
    for (e, mut cs) in q.iter_mut(world) {
        let (gold_yield, levy) = totals.get(&e).copied().unwrap_or((0, 0));
        cs.gold_yield = gold_yield;
        cs.levy = levy;
    }
}

/// Scratch: every character who leads a kingdom, via the reverse `KingdomLedBy`
/// link. A Query system now that entities live in the App world. Not scheduled.
pub fn testing(characters: Query<&KingdomLedBy>) {
    for kl in &characters {
        let _ = kl.0;
    }
}

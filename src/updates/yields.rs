//! The daily economy: every ruler's gold yield and levy recomputed from their
//! holdings, scheduled by the ECS rather than called by hand from `Ctx::tick`.

use crate::ecs::{Built, CharacterState, Holds, Leads};
use crate::resources::buildings::Buildings;
use bevy::prelude::*;

/// Recompute every character's `gold_yield` and `levy` from their holdings: a
/// leader's realm summed across its lands' buildings (gold profit less upkeep,
/// troop total); everyone else zeroed. Scheduled in `FixedUpdate`, chained
/// before [`crate::updates::tick::tick`] and
/// [`crate::updates::payout::payout`] (which pays out the freshly
/// recomputed yield on month start), and again in `Startup` so the opening
/// screen already shows what a realm renders.
///
/// One pass over the graph: character → [`Leads`] → kingdom → [`Holds`] →
/// lands → [`Built`] → [`Buildings`]. `Option<&Leads>` walks every character so
/// a ruler who leads nothing (or a landless realm) is zeroed, not left stale.
pub fn recompute_yields(
    mut characters: Query<(Option<&Leads>, &mut CharacterState)>,
    kingdoms: Query<&Holds>,
    lands: Query<&Built>,
    buildings: Res<Buildings>,
) {
    for (leads, mut cs) in &mut characters {
        let (gold_yield, levy) = leads
            .and_then(|l| kingdoms.get(l.kingdom()).ok())
            .map(|holds| {
                let (mut gold_yield, mut levy) = (0i64, 0u64);
                for land_e in holds.iter() {
                    let Ok(built) = lands.get(land_e) else {
                        continue;
                    };
                    for bid in built.0.iter() {
                        if let Some(b) = buildings.get(bid) {
                            gold_yield += b.gold_profit as i64 - b.gold_upkeep as i64;
                            levy += b.levy as u64;
                        }
                    }
                }
                (gold_yield, levy)
            })
            .unwrap_or((0, 0));
        cs.gold_yield = gold_yield;
        cs.levy = levy;
    }
}

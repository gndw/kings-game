//! The simulation context: session state — rng, player id, map selection —
//! that isn't an entity. The chronicle log is its own resource.

use crate::app::Game;
use crate::ecs::{CharacterLeads, KingdomHold, LandHolding, Registry, StringId};
use crate::rng::SimRng;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{Query, Res, ResMut};
use std::sync::{Arc, Mutex};

pub struct Ctx {
    pub seed: u64,
    pub rng: Arc<Mutex<SimRng>>,
    /// Whoever the player is playing as. An id into the character entities,
    /// resolved through `Registry` when a component is needed.
    pub player_character_id: String,
    /// The land the map selection sits on. Set on startup to the player's own seat.
    pub selected_land_id: Option<String>,
}

impl Ctx {
    /// `player` is `--player-character-id` on the command line. No default.
    pub fn new_game(seed: u64, player: &str) -> Self {
        let rng = Arc::new(Mutex::new(SimRng::new(seed)));
        Ctx {
            seed,
            rng,
            player_character_id: player.to_string(),
            selected_land_id: None,
        }
    }

    /// Resolve the opening selection: the player's own capital, via the held
    /// land of the first kingdom they lead. Multi-kingdom: only the first
    /// kingdom opens the selection.
    pub fn startup(
        mut game: ResMut<Game>,
        registry: Res<Registry>,
        character_leads: Query<&CharacterLeads>,
        kingdom_holds: Query<&KingdomHold>,
        string_ids: Query<&StringId>,
    ) {
        let Some(player_e) = registry.get(&game.ctx.player_character_id) else {
            return;
        };
        game.ctx.selected_land_id = character_leads
            .get(player_e)
            .ok()
            .and_then(|character_leads| character_leads.kingdoms().first().copied())
            .and_then(|kingdom_e| kingdom_holds.get(kingdom_e).ok())
            .map(|kingdom_hold| kingdom_hold.0)
            .and_then(|land_e| string_ids.get(land_e).ok())
            .map(|string_id| string_id.0.clone());
    }
}

// --- entity reads, `&mut World` free functions -----------------------------

/// The land to move the selection to when stepping from `from` along `dir`.
/// Picks the nearest holding that lies in that direction, penalising sideways offset.
pub fn step(world: &mut World, from_id: &str, dir: (f64, f64)) -> Option<String> {
    let from_e = world.resource::<Registry>().get(from_id)?;
    let origin = world.get::<LandHolding>(from_e)?.0;
    let mut q = world.query::<(Entity, &StringId, &LandHolding)>();
    let mut best: Option<(f64, String)> = None;
    for (e, string_id, land_holding) in q.iter(world) {
        if e == from_e {
            continue;
        }
        let (dx, dy) = (land_holding.0.0 - origin.0, land_holding.0.1 - origin.1);
        let along = dx * dir.0 + dy * dir.1;
        let perp = (dx * dir.1 - dy * dir.0).abs();
        if along > perp {
            let score = along + perp * 2.0;
            if best.as_ref().map_or(true, |(bs, _)| score < *bs) {
                best = Some((score, string_id.0.clone()));
            }
        }
    }
    best.map(|(_, id)| id)
}

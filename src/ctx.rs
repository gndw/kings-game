//! The simulation context: everything that isn't an entity. The entity world
//! lives in the App's `World`; this holds only session state — the rng, the
//! chronicle log, who the player is, and the map selection. The calendar the
//! sim runs on lives in `crate::resources`.

use crate::ecs::{CharacterState, KingdomLedBy, Land, Registry, Seat, StringId};
use crate::resources::date::Date;
use crate::rng::SimRng;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use std::sync::{Arc, Mutex};

pub struct Ctx {
    pub seed: u64,
    pub rng: Arc<Mutex<SimRng>>,
    pub chronicles: Vec<String>,
    /// Whoever the player is playing as. An id into the character entities,
    /// resolved through the [`Registry`] when a component is needed.
    ///
    /// Gold and levy are not kept here: every character has their own, as
    /// `CharacterState`, and the player is only distinguished by this id.
    pub player_character_id: String,
    pub selected_region: Option<String>,
}

impl Ctx {
    /// `player` is who to play as — `--player-character-id` on the command
    /// line, with no default: there is no such thing as the obvious character
    /// to be. It is only an id, though, and one the content doesn't have
    /// simply leaves the player bar blank rather than failing here.
    ///
    /// This no longer builds the world — entities are spawned into the App
    /// world by [`crate::ecs::populate`] afterwards, and `selected_region` is
    /// filled in by [`Ctx::finish_selection`] once those entities exist.
    pub fn new_game(seed: u64, player: &str) -> Self {
        let rng = Arc::new(Mutex::new(SimRng::new(seed)));
        let chronicles = vec![format!("{} — the chronicle begins.", Date::START)];
        Ctx {
            seed,
            rng,
            chronicles,
            player_character_id: player.to_string(),
            selected_region: None,
        }
    }

    /// Resolve the player's opening selection once the world is populated: the
    /// player's own capital, falling back to any land at all for content that
    /// doesn't contain them. Called from `main` after [`crate::ecs::populate`].
    pub fn finish_selection(&mut self, world: &World) {
        self.selected_region = player_seat_land(world, &self.player_character_id)
            .or_else(|| crate::ecs::random_land_id(world, &mut *self.rng.lock().unwrap()));
    }
}

// --- entity reads, `&World`/`&mut World` free functions ---------------------
// The UI does its reads through Bevy `Query` system params directly (see the
// `ui` modules); these are the reads the sim logic and tests need, kept here
// because they mix `Registry` lookups with component reads.

/// The player's capital, if they rule a kingdom that has one. Uses the reverse
/// [`KingdomLedBy`] link for an O(1) lookup.
pub fn player_seat_land(world: &World, player_id: &str) -> Option<String> {
    let player_e = world.resource::<Registry>().get(player_id)?;
    let kingdom_e = world.get::<KingdomLedBy>(player_e)?.0;
    let seat_e = world.get::<Seat>(kingdom_e)?.0;
    world.get::<StringId>(seat_e).map(|s| s.0.clone())
}

/// A character's mutable numbers, copied out. `reconcile` gives every defined
/// character a state entry, so this is only `None` for an id that isn't
/// defined.
pub fn character_state(world: &World, id: &str) -> Option<CharacterState> {
    let e = world.resource::<Registry>().get(id)?;
    world.get::<CharacterState>(e).map(|cs| *cs)
}

/// The land to move the selection to when stepping from `from` along `dir`
/// (a unit-ish direction). Picks the nearest holding that lies in that
/// direction, penalising sideways offset so "up" prefers straight up.
///
/// ponytail: distance heuristic over holdings, no adjacency graph. Add real
/// borders-touch adjacency in lands.ron if the picks feel wrong on odd shapes.
pub fn step(world: &mut World, from_id: &str, dir: (f64, f64)) -> Option<String> {
    let from_e = world.resource::<Registry>().get(from_id)?;
    let origin = world.get::<Land>(from_e)?.holding;
    let mut q = world.query::<(Entity, &StringId, &Land)>();
    let mut best: Option<(f64, String)> = None;
    for (e, sid, l) in q.iter(world) {
        if e == from_e {
            continue;
        }
        let (dx, dy) = (l.holding.0 - origin.0, l.holding.1 - origin.1);
        let along = dx * dir.0 + dy * dir.1;
        // Perpendicular component: how far off-axis the candidate sits.
        let perp = (dx * dir.1 - dy * dir.0).abs();
        if along > perp {
            let score = along + perp * 2.0;
            if best.as_ref().map_or(true, |(bs, _)| score < *bs) {
                best = Some((score, sid.0.clone()));
            }
        }
    }
    best.map(|(_, id)| id)
}

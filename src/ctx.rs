//! The simulation context: everything that isn't an entity. The entity world
//! lives in the App's `World`; this holds only session state — the rng, the
//! chronicle log, who the player is, and the map selection. The calendar the
//! sim runs on lives in `crate::resources`.

use crate::app::Game;
use crate::ecs::{Land, Leads, Registry, Seat, StringId};
use crate::resources::date::Date;
use crate::rng::SimRng;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::{Query, Res, ResMut};
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
    /// The land the map selection sits on, as a `StringId`. Set on startup to
    /// the player's own seat by [`Ctx::startup`]; arrow keys move it via [`step`].
    pub selected_land_id: Option<String>,
}

impl Ctx {
    /// `player` is who to play as — `--player-character-id` on the command
    /// line, with no default: there is no such thing as the obvious character
    /// to be. It is only an id, though, and one the content doesn't have
    /// simply leaves the player bar blank rather than failing here.
    ///
    /// This builds only the session state — entities are spawned into the App
    /// world by [`crate::ecs::populate`], and `selected_land_id` is set by
    /// [`Ctx::startup`] in the `Startup` schedule once those entities exist.
    pub fn new_game(seed: u64, player: &str) -> Self {
        let rng = Arc::new(Mutex::new(SimRng::new(seed)));
        let chronicles = vec![format!("{} — the chronicle begins.", Date::START)];
        Ctx {
            seed,
            rng,
            chronicles,
            player_character_id: player.to_string(),
            selected_land_id: None,
        }
    }

    /// Resolve the opening selection: the player's own capital, via their
    /// kingdom's [`Seat`]. Runs in the `Startup` schedule, after
    /// [`crate::ecs::populate`] has spawned the entities. Left `None` if the
    /// player leads no kingdom with a seat.
    pub fn startup(
        mut game: ResMut<Game>,
        registry: Res<Registry>,
        leads: Query<&Leads>,
        seats: Query<&Seat>,
        string_ids: Query<&StringId>,
    ) {
        let Some(player_e) = registry.get(&game.ctx.player_character_id) else {
            return;
        };
        game.ctx.selected_land_id = leads
            .get(player_e)
            .ok()
            .and_then(|l| seats.get(l.kingdom()).ok())
            .and_then(|s| string_ids.get(s.0).ok())
            .map(|s| s.0.clone());
    }
}

// --- entity reads, `&mut World` free functions -----------------------------
// The UI does its reads through Bevy `Query` system params directly (see the
// `ui` modules); `step` mixes `Registry` lookup with component reads and so
// runs as an exclusive system.

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

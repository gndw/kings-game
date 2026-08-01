//! The simulation context: the hecs world plus everything that isn't an entity.
//! The calendar it runs on lives in `crate::date`.

use crate::date::Date;
use crate::map::{Character, Map, Yield};
use crate::rng::SimRng;
use hecs::World;
use std::sync::{Arc, Mutex};

/// ponytail: hardcoded until there's a start screen to pick a character from.
/// Move it into the mod data at that point, not before.
const PLAYER_CHARACTER_ID: &str = "char-tywin";

pub struct Ctx {
    pub world: World,
    pub map: Map,
    pub date: Date,
    pub seed: u64,
    pub tick_count: u64,
    pub rng: Arc<Mutex<SimRng>>,
    pub chronicles: Vec<String>,
    /// Whoever the player is playing as. Ids into `Map::characters`.
    ///
    /// Gold and levy are not kept here: every character has their own, on
    /// `Character`, and the player is only distinguished by this id.
    pub player_character_id: String,
    pub selected_region: Option<String>,
}

impl Ctx {
    pub fn new_game(seed: u64, map: Map) -> Self {
        let player_character_id = PLAYER_CHARACTER_ID.to_string();
        let mut ctx = Ctx {
            // Open on the player's own capital. Falls back to any land at all
            // for a map that doesn't happen to contain them — the empty map
            // the clock tests use, or a mod that dropped the character.
            selected_region: map
                .kingdom_led_by(&player_character_id)
                .map(|k| k.seat_land_id.clone())
                .or_else(|| map.random_land_id()),
            player_character_id,
            map,
            world: World::new(),
            date: Date {
                year: 1066,
                month: 1,
                day: 1,
            },
            seed,
            tick_count: 0,
            rng: Arc::new(Mutex::new(SimRng::new(seed))),
            chronicles: Vec::new(),
        };
        ctx.chronicles
            .push(format!("{} — the chronicle begins.", ctx.date));
        ctx
    }

    /// The character the player is, if the map still contains them.
    pub fn player_character(&self) -> Option<&Character> {
        self.map.character(&self.player_character_id)
    }

    /// What a character's holdings add up to. All zeroes unless they lead a
    /// kingdom — which is what confines gold and levy to rulers.
    pub fn yield_for(&self, character_id: &str) -> Yield {
        self.map
            .kingdom_led_by(character_id)
            .map(|k| self.map.kingdom_yield(k))
            .unwrap_or_default()
    }

    /// One simulated day. Systems hook in here.
    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.date.advance(&self.map.calendar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::parse;

    #[test]
    fn a_new_game_opens_on_the_players_capital() {
        // The player's kingdom is listed second on purpose: picking the first
        // one would pass a weaker test.
        let map = parse(
            r#"(
            border: (x0: 0, y0: 0, x1: 10, y1: 10),
            lands: [
                (id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)]),
                (id: "l2", name: "L2", holding: (5, 5), borders: [(5, 5), (6, 6)]),
            ],
            houses: [(id: "h1", name: "H1")],
            characters: [
                (id: "other", name: "other", house_id: "h1", age: 40),
                (id: "char-tywin", name: "tywin", house_id: "h1", age: 57),
            ],
            kingdoms: [
                (id: "k-other", leader_character_id: "other", seat_land_id: "l1", land_ids: ["l1"]),
                (id: "k-tywin", leader_character_id: "char-tywin", seat_land_id: "l2", land_ids: ["l2"]),
            ],
        )"#,
        )
        .unwrap();
        let ctx = Ctx::new_game(1, map);
        assert_eq!(ctx.player_character_id, PLAYER_CHARACTER_ID);
        assert_eq!(ctx.selected_region.as_deref(), Some("l2"));
    }

    #[test]
    fn a_map_without_the_player_still_selects_something() {
        let map = parse(
            r#"(border: (x0: 0, y0: 0, x1: 10, y1: 10),
               lands: [(id: "only", name: "O", holding: (1, 1), borders: [(1, 1), (2, 2)])])"#,
        )
        .unwrap();
        assert_eq!(
            Ctx::new_game(1, map).selected_region.as_deref(),
            Some("only")
        );
        // ...and an empty map has nothing to select at all.
        assert!(Ctx::new_game(1, Map::default()).selected_region.is_none());
    }
}

//! The simulation context: the hecs world plus everything that isn't an entity.
//! The calendar it runs on lives in `crate::date`.

use crate::content::{Character, Content};
use crate::date::Date;
use crate::rng::SimRng;
use crate::state::{CharacterState, State};
use hecs::World;
use std::sync::{Arc, Mutex};

pub struct Ctx {
    pub world: World,
    /// Everything the mods define: geometry, calendar, the roster. Read-only.
    pub content: Content,
    /// Everything that changes and would go in a save file: realms, holdings,
    /// and every character's gold and levy. Keyed by the ids in `content`.
    pub state: State,
    pub date: Date,
    pub seed: u64,
    pub tick_count: u64,
    pub rng: Arc<Mutex<SimRng>>,
    pub chronicles: Vec<String>,
    /// Whoever the player is playing as. Ids into `Content::characters`.
    ///
    /// Gold and levy are not kept here: every character has their own, on
    /// `Character`, and the player is only distinguished by this id.
    pub player_character_id: String,
    pub selected_region: Option<String>,
}

impl Ctx {
    /// `player` is who to play as — `--player-character-id` on the command
    /// line, with no default: there is no such thing as the obvious character
    /// to be. It is only an id, though, and one the content doesn't have
    /// simply leaves the player bar blank rather than failing here.
    pub fn new_game(seed: u64, content: Content, state: State, player: &str) -> Self {
        let player_character_id = player.to_string();
        let mut ctx = Ctx {
            // Open on the player's own capital. Falls back to any land at all
            // for content that doesn't happen to contain them — the empty
            // default the clock tests use, or a mod that dropped the character.
            selected_region: state
                .kingdom_led_by(&player_character_id)
                .map(|k| k.seat_land_id.clone())
                .or_else(|| content.random_land_id()),
            player_character_id,
            content,
            state,
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
        self.content.character(&self.player_character_id)
    }

    /// Their gold and levy. `reconcile` gives every defined character a state
    /// entry, so this is only ever `None` for a character who isn't defined.
    pub fn player_state(&self) -> Option<&CharacterState> {
        self.state.character(&self.player_character_id)
    }

    /// One simulated day. Systems hook in here.
    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.date.advance(&self.content.calendar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::parse;
    use crate::state::parse_file;

    /// Two rulers, whose kingdoms are listed the other way round from their
    /// characters — picking the first of either would pass a weaker test.
    fn two_realms() -> (Content, State) {
        let map = parse(
            r#"(
            border: (x0: 0, y0: 0, x1: 10, y1: 10),
            lands: [
                (id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)]),
                (id: "l2", name: "L2", holding: (5, 5), borders: [(5, 5), (6, 6)]),
            ],
            houses: [(id: "h1", name: "H1")],
            characters: [
                (id: "other", name: "other", house_id: "h1"),
                (id: "char-tywin", name: "tywin", house_id: "h1"),
            ],
        )"#,
        )
        .unwrap();
        let state = parse_file(
            r#"(kingdoms: [
                (id: "k-other", leader_character_id: "other", seat_land_id: "l1", land_ids: ["l1"]),
                (id: "k-tywin", leader_character_id: "char-tywin", seat_land_id: "l2", land_ids: ["l2"]),
            ])"#,
        )
        .unwrap();
        (map, state)
    }

    /// Whoever `--player-character-id` names is who you are, and the game
    /// opens on *their* capital — run the same world twice as two different
    /// rulers and only the player changes.
    #[test]
    fn a_new_game_opens_on_the_players_capital() {
        let (map, state) = two_realms();
        let ctx = Ctx::new_game(1, map, state, "char-tywin");
        assert_eq!(ctx.player_character_id, "char-tywin");
        assert_eq!(ctx.selected_region.as_deref(), Some("l2"));

        let (map, state) = two_realms();
        let ctx = Ctx::new_game(1, map, state, "other");
        assert_eq!(ctx.player_character_id, "other");
        assert_eq!(ctx.selected_region.as_deref(), Some("l1"));
    }

    /// A player the content doesn't have is survivable, not fatal: `main`
    /// refuses the id up front, but a mod that drops a character mid-campaign
    /// must not take the map with it.
    #[test]
    fn a_map_without_the_player_still_selects_something() {
        let map = parse(
            r#"(border: (x0: 0, y0: 0, x1: 10, y1: 10),
               lands: [(id: "only", name: "O", holding: (1, 1), borders: [(1, 1), (2, 2)])])"#,
        )
        .unwrap();
        assert_eq!(
            Ctx::new_game(1, map, State::default(), "char-tywin")
                .selected_region
                .as_deref(),
            Some("only")
        );
        // ...and an empty map has nothing to select at all.
        assert!(
            Ctx::new_game(1, Content::default(), State::default(), "char-tywin")
                .selected_region
                .is_none()
        );
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::parse;
    use crate::ecs;
    use crate::resources::calendar::Calendar;
    use crate::state::parse_file;
    use crate::updates::payout::payout;
    use crate::updates::yields::recompute;

    /// Build a populated world plus a `Ctx` for `player`, mirroring what `main`
    /// does: populate, seed the economy, resolve the opening selection.
    fn setup(
        seed: u64,
        content: crate::content::Content,
        state: crate::state::State,
        player: &str,
    ) -> (World, Ctx) {
        let mut world = World::new();
        ecs::populate(&mut world, content, state);
        // Seed yields so the opening screen shows what each realm renders.
        recompute(&mut world);
        let mut ctx = Ctx::new_game(seed, player);
        ctx.finish_selection(&world);
        (world, ctx)
    }

    /// Two rulers, whose kingdoms are listed the other way round from their
    /// characters — picking the first of either would pass a weaker test.
    fn two_realms() -> (crate::content::Content, crate::state::State) {
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
        let (_world, ctx) = setup(1, map, state, "char-tywin");
        assert_eq!(ctx.player_character_id, "char-tywin");
        assert_eq!(ctx.selected_region.as_deref(), Some("l2"));

        let (map, state) = two_realms();
        let (_world, ctx) = setup(1, map, state, "other");
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
        let (_world, ctx) = setup(1, map, crate::state::State::default(), "char-tywin");
        assert_eq!(ctx.selected_region.as_deref(), Some("only"));

        // ...and an empty map has nothing to select at all.
        let mut world = World::new();
        ecs::populate(
            &mut world,
            crate::content::Content::default(),
            crate::state::State::default(),
        );
        assert!(player_seat_land(&world, "char-tywin").is_none());
    }

    /// Selection stepping over the entity world.
    #[test]
    fn steps_between_lands() {
        let content = parse(
            r#"(
                border: (x0: 0, y0: 0, x1: 10, y1: 10),
                lands: [
                    (id: "mid", name: "mid", holding: (5, 5), borders: [(5, 5), (5, 5)]),
                    (id: "east", name: "east", holding: (8, 5), borders: [(8, 5), (8, 5)]),
                    (id: "far_east", name: "far_east", holding: (9, 5), borders: [(9, 5), (9, 5)]),
                    (id: "north", name: "north", holding: (5, 9), borders: [(5, 9), (5, 9)]),
                ],
            )"#,
        )
        .unwrap();
        let (mut world, _ctx) = setup(1, content, crate::state::State::default(), "nobody");
        assert_eq!(step(&mut world, "mid", (1.0, 0.0)).as_deref(), Some("east"));
        assert_eq!(
            step(&mut world, "east", (1.0, 0.0)).as_deref(),
            Some("far_east")
        );
        assert_eq!(
            step(&mut world, "mid", (0.0, 1.0)).as_deref(),
            Some("north")
        );
        assert_eq!(step(&mut world, "north", (0.0, 1.0)), None);
        // Nothing west of mid, and an unknown land can't step.
        assert_eq!(step(&mut world, "mid", (-1.0, 0.0)), None);
        assert_eq!(step(&mut world, "nowhere", (1.0, 0.0)), None);
    }

    /// Two rulers and a landless character — the economy's cast. See
    /// [`the_economy_pays_out_monthly`].
    const ECON: &str = r#"(
        border: (x0: 0, y0: 0, x1: 10, y1: 10),
        buildings: [
            (id: "b-barracks", name: "barracks", gold_upkeep: 5, levy: 50),
            (id: "b-mill", name: "mill", gold_profit: 6),
            (id: "b-market", name: "market", gold_profit: 10),
        ],
        lands: [
            (id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)]),
            (id: "l2", name: "L2", holding: (5, 5), borders: [(5, 5), (6, 6)]),
            (id: "l3", name: "L3", holding: (8, 8), borders: [(8, 8), (9, 9)]),
        ],
        houses: [(id: "h1", name: "H1")],
        characters: [
            (id: "char-tywin",  name: "tywin",  house_id: "h1"),
            (id: "char-jon",    name: "jon",    house_id: "h1"),
            (id: "char-hoster", name: "hoster", house_id: "h1"),
            (id: "char-lysa",   name: "lysa",   house_id: "h1"),
        ],
    )"#;

    /// Where that world starts: who holds what, and what's in the treasuries.
    const ECON_STATE: &str = r#"(
        lands: [
            (id: "l1", building_ids: ["b-barracks", "b-mill"]),
            (id: "l2", building_ids: ["b-market"]),
            (id: "l3", building_ids: ["b-barracks"]),
        ],
        characters: [
            (id: "char-tywin",  age: 57),
            (id: "char-jon",    age: 66, gold: 100),
            (id: "char-hoster", age: 48, gold: 3),
            (id: "char-lysa",   age: 32),
        ],
        kingdoms: [
            (id: "k1", leader_character_id: "char-tywin",  seat_land_id: "l1", land_ids: ["l1"]),
            (id: "k2", leader_character_id: "char-jon",    seat_land_id: "l2", land_ids: ["l2"]),
            (id: "k3", leader_character_id: "char-hoster", seat_land_id: "l3", land_ids: ["l3"]),
        ],
    )"#;

    /// A character's `(gold, levy)`.
    fn purse(world: &World, id: &str) -> (i64, u64) {
        let c = character_state(world, id).unwrap();
        (c.gold, c.levy)
    }

    /// A character's monthly gold yield, signed.
    fn gold_yield(world: &World, id: &str) -> i64 {
        character_state(world, id).unwrap().gold_yield
    }

    /// Gold yield and levy are the sim's now: `setup` seeds them and each tick
    /// holds them, with the payout landing on the first of the month.
    #[test]
    fn the_economy_pays_out_monthly() {
        let map = parse(ECON).unwrap();
        let state = parse_file(ECON_STATE).unwrap();
        let (mut world, mut ctx) = setup(1, map, state, "char-tywin");
        let calendar = Calendar::default();
        let mut date = Date::START;

        // `setup` seeds the yields, so the opening screen already shows the
        // levy and the income — treasuries untouched.
        assert_eq!(purse(&world, "char-tywin"), (0, 50));
        assert_eq!(
            gold_yield(&world, "char-tywin"),
            1,
            "6 profit less 5 upkeep"
        );
        assert_eq!(
            purse(&world, "char-jon"),
            (100, 0),
            "no barracks, no taxes yet"
        );
        assert_eq!(gold_yield(&world, "char-jon"), 10, "nothing to keep up");
        assert_eq!(
            gold_yield(&world, "char-hoster"),
            -5,
            "upkeep and no earnings"
        );
        assert_eq!(gold_yield(&world, "char-lysa"), 0, "landless earns nothing");

        // A day holds the levy and pays no taxes.
        crate::updates::tick::advance(&mut date, &calendar);
        assert_eq!(
            purse(&world, "char-tywin").1,
            50,
            "levy holds on the first day"
        );
        assert_eq!(purse(&world, "char-jon").1, 0, "a realm with no barracks");
        assert_eq!(purse(&world, "char-tywin").0, 0, "no taxes until the 1st");

        // Day 1 of month 2 is tick 30 on the default 30-day calendar.
        for _ in 1..30 {
            crate::updates::tick::advance(&mut date, &calendar);
        }
        assert_eq!((date.month, date.day), (2, 1));
        // The payout is a separate system now: it fires on month start, chained
        // after `tick` in `FixedUpdate`, so drive it by hand here.
        payout(&mut world, &mut ctx);
        assert_eq!(
            purse(&world, "char-tywin"),
            (1, 50),
            "the barracks eats 5 of the mill's 6"
        );
        assert_eq!(
            purse(&world, "char-jon"),
            (110, 0),
            "every ruler collects, not just the player"
        );
        assert_eq!(
            purse(&world, "char-hoster"),
            (-2, 50),
            "3 gold less 5 of upkeep — a treasury goes past zero, not to it"
        );
        assert_eq!(
            purse(&world, "char-lysa"),
            (0, 0),
            "leading no kingdom earns and raises nothing"
        );

        // Only the player's finances are chronicled — hoster's deficit is real
        // but goes unreported.
        assert_eq!(
            ctx.chronicles
                .iter()
                .filter(|l| l.contains("gold in taxes"))
                .count(),
            1
        );
        assert!(ctx.chronicles.iter().any(|l| l.contains("1 gold in taxes")));
        assert!(!ctx.chronicles.iter().any(|l| l.contains("gold short")));

        // A second month, a second payment, and the levies hold steady.
        for _ in 0..30 {
            crate::updates::tick::advance(&mut date, &calendar);
        }
        payout(&mut world, &mut ctx);
        assert_eq!(purse(&world, "char-tywin"), (2, 50));
        assert_eq!(purse(&world, "char-jon"), (120, 0));
        assert_eq!(
            purse(&world, "char-hoster"),
            (-7, 50),
            "debt keeps deepening"
        );
    }
}

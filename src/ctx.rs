//! The simulation context: the entity world plus everything that isn't an entity.
//! The calendar it runs on lives in `crate::resources`.

use crate::content::{Border, Content};
use crate::resources::date::Date;
use crate::ecs::{
    self, BuildingData, Built, Character, CharacterData, CharacterState, Globals, House, HouseOf,
    EntityIndex, Holds, KingdomData, Land, LandData, LedBy, PlayerSummary, Registry, Seat, StringId,
};
use crate::rng::SimRng;
use crate::state::State;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use std::sync::{Arc, Mutex};

pub struct Ctx {
    pub world: World,
    /// The edge of the world. Content, but not entity-shaped.
    pub border: Border,
    /// Simulated days per real second. Content, but not entity-shaped.
    pub speeds: Vec<u32>,
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
    /// Content and state are consumed into the entity world here; afterwards
    /// `Ctx` holds no `IndexMap`s.
    pub fn new_game(seed: u64, content: Content, state: State, player: &str) -> Self {
        let player_character_id = player.to_string();
        let rng = Arc::new(Mutex::new(SimRng::new(seed)));
        let mut world = World::new();
        world.insert_resource(Registry::new());
        let Globals { border, speeds } = ecs::populate(&mut world, content, state);
        // Open on the player's own capital. Falls back to any land at all for
        // content that doesn't happen to contain them — the empty default the
        // clock tests use, or a mod that dropped the character.
        let selected_region = player_seat_land(&world, &player_character_id)
            .or_else(|| ecs::random_land_id(&world, &mut *rng.lock().unwrap()));
        let mut ctx = Ctx {
            world,
            border,
            speeds,
            seed,
            rng,
            chronicles: Vec::new(),
            player_character_id,
            selected_region,
        };
        ctx.chronicles
            .push(format!("{} — the chronicle begins.", Date::START));
        // The economy is the sim's now, not a mod's: seed every ruler's yield
        // and levy so the opening screen shows what their realm renders, the
        // way `on_startup` once did. The `recompute_yields` system keeps it
        // fresh each day thereafter.
        ctx
    }

    // --- id / entity plumbing ------------------------------------------------

    fn player_entity(&self) -> Option<Entity> {
        self.world
            .resource::<Registry>()
            .get(&self.player_character_id)
    }

    /// An entity's id, or "" if it has none. The cheap reverse of [`Registry`].
    fn entity_id(&self, e: Entity) -> String {
        self.world
            .get::<StringId>(e)
            .map(|s| s.0.clone())
            .unwrap_or_default()
    }

    /// A character's house name, if they belong to a house that exists.
    fn house_name_of(&self, char_e: Entity) -> Option<String> {
        let house_e = self.world.get::<HouseOf>(char_e)?.0;
        Some(self.world.get::<House>(house_e)?.name.clone())
    }

    // --- per-entity reads (owned snapshots for the UI and tests) -------------

    /// A character's mutable numbers, copied out. `reconcile` gives every
    /// defined character a state entry, so this is only `None` for an id that
    /// isn't defined.
    pub fn character_state(&self, id: &str) -> Option<CharacterState> {
        let e = self.world.resource::<Registry>().get(id)?;
        self.world.get::<CharacterState>(e).map(|cs| *cs)
    }

    /// Everything the resource bar shows: the player's name, house, treasury,
    /// monthly yield and levy. `None` when the player isn't in the world, which
    /// leaves the bar blank rather than showing zeroes.
    pub fn player_summary(&self) -> Option<PlayerSummary> {
        let e = self.player_entity()?;
        let ch = self.world.get::<Character>(e)?;
        let cs = self.world.get::<CharacterState>(e)?;
        let house = self.house_name_of(e).unwrap_or_default();
        Some(PlayerSummary {
            name: ch.name.clone(),
            house,
            gold: cs.gold,
            gold_yield: cs.gold_yield,
            levy: cs.levy,
        })
    }

    /// A character's name, house and age — the legend's ruler line.
    pub fn character_brief(&self, id: &str) -> Option<CharacterData> {
        let e = self.world.resource::<Registry>().get(id)?;
        let ch = self.world.get::<Character>(e)?;
        let house_name = self.house_name_of(e).unwrap_or_default();
        let age = self
            .world
            .get::<CharacterState>(e)
            .map(|cs| cs.age)
            .unwrap_or(0);
        Some(CharacterData {
            id: id.to_string(),
            name: ch.name.clone(),
            house_name,
            age,
        })
    }

    /// What stands in `land_id`, in build order. Each entry carries its own
    /// gold/levy numbers so the legend can total and list them in one pass.
    pub fn buildings_in_land(&self, land_id: &str) -> Vec<BuildingData> {
        let Some(e) = self.world.resource::<Registry>().get(land_id) else {
            return Vec::new();
        };
        let Some(built) = self.world.get::<Built>(e) else {
            return Vec::new();
        };
        built
            .0
            .iter()
            .filter_map(|&be| {
                let bd = self.world.get::<ecs::Building>(be)?;
                let sid = self.world.get::<StringId>(be)?;
                Some(BuildingData {
                    id: sid.0.clone(),
                    name: bd.name.clone(),
                    gold_profit: bd.gold_profit,
                    gold_upkeep: bd.gold_upkeep,
                    levy: bd.levy,
                })
            })
            .collect()
    }

    /// The kingdom holding `land_id`, if any.
    pub fn kingdom_of_land(&self, land_id: &str) -> Option<KingdomData> {
        let land_e = self.world.resource::<Registry>().get(land_id)?;
        let kingdoms = self.world.resource::<EntityIndex>().kingdoms.clone();
        let &kingdom_e = kingdoms
            .iter()
            .find(|&&ke| self.world.get::<Holds>(ke).is_some_and(|h| h.0.contains(&land_e)))?;
        // A kingdom that holds the land but lacks a resolved seat or leader is,
        // like the old query that required all three components, no match.
        let seat_e = self.world.get::<Seat>(kingdom_e)?.0;
        let leader_e = self.world.get::<LedBy>(kingdom_e)?.0;
        let land_es = self.world.get::<Holds>(kingdom_e)?.0.clone();
        Some(KingdomData {
            id: self.entity_id(kingdom_e),
            seat_land_id: self.entity_id(seat_e),
            leader_character_id: self.entity_id(leader_e),
            land_ids: land_es.iter().map(|&e| self.entity_id(e)).collect(),
        })
    }

    /// The ids of the lands the player's kingdom holds. Empty for a player who
    /// rules nothing.
    pub fn player_holds(&self) -> Vec<String> {
        let Some(player_e) = self.player_entity() else {
            return Vec::new();
        };
        let kingdoms = self.world.resource::<EntityIndex>().kingdoms.clone();
        let kingdom_e = kingdoms
            .iter()
            .find(|&&ke| self.world.get::<LedBy>(ke).is_some_and(|l| l.0 == player_e))
            .copied();
        match kingdom_e {
            Some(ke) => self
                .world
                .get::<Holds>(ke)
                .map(|h| h.0.iter().copied().map(|e| self.entity_id(e)).collect())
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Every land, in content order, with its geometry. What the map iterates.
    pub fn lands_ordered(&self) -> Vec<LandData> {
        let lands = self.world.resource::<EntityIndex>().lands.clone();
        lands
            .iter()
            .filter_map(|&e| {
                let sid = self.world.get::<StringId>(e)?;
                let l = self.world.get::<Land>(e)?;
                Some(LandData {
                    id: sid.0.clone(),
                    name: l.name.clone(),
                    borders: l.borders.clone(),
                    holding: l.holding,
                })
            })
            .collect()
    }

    /// A land's name, if it exists.
    pub fn land_name(&self, id: &str) -> Option<String> {
        let e = self.world.resource::<Registry>().get(id)?;
        self.world.get::<Land>(e).map(|l| l.name.clone())
    }

    /// The land to move the selection to when stepping from `from` along `dir`
    /// (a unit-ish direction). Picks the nearest holding that lies in that
    /// direction, penalising sideways offset so "up" prefers straight up.
    ///
    /// ponytail: distance heuristic over holdings, no adjacency graph. Add real
    /// borders-touch adjacency in lands.ron if the picks feel wrong on odd shapes.
    pub fn step(&self, from: &str, dir: (f64, f64)) -> Option<String> {
        let from_e = self.world.resource::<Registry>().get(from)?;
        let origin = self.world.get::<Land>(from_e)?.holding;
        let lands = self.world.resource::<EntityIndex>().lands.clone();
        let mut best: Option<(f64, String)> = None;
        for &e in &lands {
            if e == from_e {
                continue;
            }
            let l = self.world.get::<Land>(e)?;
            let (dx, dy) = (l.holding.0 - origin.0, l.holding.1 - origin.1);
            let along = dx * dir.0 + dy * dir.1;
            // Perpendicular component: how far off-axis the candidate sits.
            let perp = (dx * dir.1 - dy * dir.0).abs();
            if along > perp {
                let score = along + perp * 2.0;
                if best.as_ref().map_or(true, |(bs, _)| score < *bs) {
                    best = Some((score, self.world.get::<StringId>(e)?.0.clone()));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// A random land's id, or `None` when there are none. Drawn from the seeded
    /// RNG so it replays.
    pub fn random_land_id(&self) -> Option<String> {
        ecs::random_land_id(&self.world, &mut *self.rng.lock().unwrap())
    }
}

/// The player's capital, if they rule a kingdom that has one.
fn player_seat_land(world: &World, player_id: &str) -> Option<String> {
    let player_e = world.resource::<Registry>().get(player_id)?;
    let kingdoms = world.resource::<EntityIndex>().kingdoms.clone();
    let seat_e = kingdoms
        .iter()
        .find(|&&ke| world.get::<LedBy>(ke).is_some_and(|l| l.0 == player_e))
        .and_then(|&ke| world.get::<Seat>(ke).map(|s| s.0))?;
    world.get::<StringId>(seat_e).map(|s| s.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::parse;
    use crate::resources::calendar::Calendar;
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

    /// Selection stepping, now over the entity world instead of an `IndexMap`.
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
        let ctx = Ctx::new_game(1, content, State::default(), "nobody");
        assert_eq!(ctx.step("mid", (1.0, 0.0)).as_deref(), Some("east"));
        assert_eq!(ctx.step("east", (1.0, 0.0)).as_deref(), Some("far_east"));
        assert_eq!(ctx.step("mid", (0.0, 1.0)).as_deref(), Some("north"));
        assert_eq!(ctx.step("north", (0.0, 1.0)), None);
        // Nothing west of mid, and an unknown land can't step.
        assert_eq!(ctx.step("mid", (-1.0, 0.0)), None);
        assert_eq!(ctx.step("nowhere", (1.0, 0.0)), None);
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
    fn purse(ctx: &Ctx, id: &str) -> (i64, u64) {
        let c = ctx.character_state(id).unwrap();
        (c.gold, c.levy)
    }

    /// A character's monthly gold yield, signed.
    fn gold_yield(ctx: &Ctx, id: &str) -> i64 {
        ctx.character_state(id).unwrap().gold_yield
    }

    /// Gold yield and levy are the sim's now: `new_game` seeds them and each
    /// tick holds them, with the payout landing on the first of the month.
    #[test]
    fn the_economy_pays_out_monthly() {
        let map = parse(ECON).unwrap();
        let state = parse_file(ECON_STATE).unwrap();
        let mut ctx = Ctx::new_game(1, map, state, "char-tywin");
        let calendar = Calendar::default();
        let mut date = Date::START;

        // `new_game` seeds the yields, so the opening screen already shows the
        // levy and the income — treasuries untouched.
        assert_eq!(purse(&ctx, "char-tywin"), (0, 50));
        assert_eq!(gold_yield(&ctx, "char-tywin"), 1, "6 profit less 5 upkeep");
        assert_eq!(purse(&ctx, "char-jon"), (100, 0), "no barracks, no taxes yet");
        assert_eq!(gold_yield(&ctx, "char-jon"), 10, "nothing to keep up");
        assert_eq!(gold_yield(&ctx, "char-hoster"), -5, "upkeep and no earnings");
        assert_eq!(gold_yield(&ctx, "char-lysa"), 0, "landless earns nothing");

        // A day holds the levy and pays no taxes.
        crate::updates::tick::advance(&mut date, &calendar);
        assert_eq!(purse(&ctx, "char-tywin").1, 50, "levy holds on the first day");
        assert_eq!(purse(&ctx, "char-jon").1, 0, "a realm with no barracks");
        assert_eq!(purse(&ctx, "char-tywin").0, 0, "no taxes until the 1st");

        // Day 1 of month 2 is tick 30 on the default 30-day calendar.
        for _ in 1..30 {
            crate::updates::tick::advance(&mut date, &calendar);
        }
        assert_eq!((date.month, date.day), (2, 1));
        // The payout is a separate system now: it fires on month start, chained
        // after `tick` in `FixedUpdate`, so drive it by hand here.
        crate::updates::payout::payout(&mut ctx);
        assert_eq!(
            purse(&ctx, "char-tywin"),
            (1, 50),
            "the barracks eats 5 of the mill's 6"
        );
        assert_eq!(
            purse(&ctx, "char-jon"),
            (110, 0),
            "every ruler collects, not just the player"
        );
        assert_eq!(
            purse(&ctx, "char-hoster"),
            (-2, 50),
            "3 gold less 5 of upkeep — a treasury goes past zero, not to it"
        );
        assert_eq!(
            purse(&ctx, "char-lysa"),
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
        crate::updates::payout::payout(&mut ctx);
        assert_eq!(purse(&ctx, "char-tywin"), (2, 50));
        assert_eq!(purse(&ctx, "char-jon"), (120, 0));
        assert_eq!(purse(&ctx, "char-hoster"), (-7, 50), "debt keeps deepening");
    }
}

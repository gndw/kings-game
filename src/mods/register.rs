//! The script surface: every `ctx.thing` a mod may read, and nothing else. The
//! calls that *write* are registered by [`super::effects`], next to the effects
//! they queue.
//!
//! Keep [`ScriptCtx`] and the README's two tables in step with this list.

use super::ScriptCtx;
use rhai::{Array, Dynamic, Engine, ImmutableString};

/// A Rhai array of id strings — what every list the scripts read looks like.
fn ids<'a>(it: impl Iterator<Item = &'a str>) -> Array {
    it.map(|id| Dynamic::from(ImmutableString::from(id)))
        .collect()
}

pub(super) fn script_ctx(engine: &mut Engine) {
    engine
        .register_type_with_name::<ScriptCtx>("Ctx")
        .register_get("year", |c: &mut ScriptCtx| c.year)
        .register_get("month", |c: &mut ScriptCtx| c.month)
        .register_get("day", |c: &mut ScriptCtx| c.day)
        .register_get("tick", |c: &mut ScriptCtx| c.tick)
        .register_get("land", |c: &mut ScriptCtx| c.land.clone())
        .register_get("player", |c: &mut ScriptCtx| c.player.clone())
        .register_get("characters", |c: &mut ScriptCtx| {
            ids(c.realms.characters.ids.iter().map(String::as_str))
        })
        // Per-character reads. An unknown id reads as all zeroes rather
        // than erroring — a script looping the characters can't hit one.
        .register_fn("gold", |c: &mut ScriptCtx, id: ImmutableString| {
            c.realms.characters.get(&id).gold
        })
        .register_fn("levy", |c: &mut ScriptCtx, id: ImmutableString| {
            c.realms.characters.get(&id).levy as i64
        })
        // The world's shape. Whether a character rules anything, and what
        // their holdings add up to, is a script's sum to do — see
        // `mods/base/character_levy.rhai`.
        .register_get("kingdoms", |c: &mut ScriptCtx| {
            ids(c.realms.kingdoms.iter().map(|k| k.id.as_str()))
        })
        .register_fn(
            "kingdom_leader",
            |c: &mut ScriptCtx, id: ImmutableString| {
                c.realms
                    .kingdom(&id)
                    .map(|k| k.leader.clone())
                    .unwrap_or_default()
            },
        )
        .register_fn(
            "kingdom_lands",
            |c: &mut ScriptCtx, id: ImmutableString| match c.realms.kingdom(&id) {
                Some(k) => ids(k.land_ids.iter().map(String::as_str)),
                None => Array::new(),
            },
        )
        .register_fn(
            "land_buildings",
            |c: &mut ScriptCtx, id: ImmutableString| match c.realms.buildings.get(id.as_str()) {
                Some(b) => ids(b.iter().map(String::as_str)),
                None => Array::new(),
            },
        )
        .register_fn("building_levy", |c: &mut ScriptCtx, id: ImmutableString| {
            c.realms.building(&id).levy as i64
        })
        .register_fn(
            "building_gold_profit",
            |c: &mut ScriptCtx, id: ImmutableString| c.realms.building(&id).gold_profit as i64,
        )
        .register_fn(
            "building_gold_upkeep",
            |c: &mut ScriptCtx, id: ImmutableString| c.realms.building(&id).gold_upkeep as i64,
        )
        .register_fn("rand", |c: &mut ScriptCtx| c.rand());
    super::effects::register(engine);
}

#[cfg(test)]
mod tests {
    use super::super::load;
    use super::super::testkit::*;
    use crate::ctx::Ctx;

    /// Two rulers and a landless character.
    ///
    /// - `char-tywin` (the player) holds `l1`: 50 levy, 6 profit, 5 upkeep — so
    ///   a net yield of 1, which is what makes the upkeep subtraction visible.
    /// - `char-jon` holds `l2`: 0 levy, 10 profit, no upkeep, starts on 100 gold.
    /// - `char-hoster` holds `l3`: a barracks and nothing to pay for it, so his
    ///   realm runs at -5 a month and he goes into debt.
    /// - `char-lysa` leads nothing at all.
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
        let c = ctx.state.character(id).unwrap();
        (c.gold, c.levy)
    }

    /// What their holdings net per month, profit less upkeep. Signed.
    fn gold_yield(ctx: &Ctx, id: &str) -> i64 {
        ctx.state.character(id).unwrap().gold_yield
    }

    /// The base scripts do their own sums off this surface, so this covers both:
    /// break a registration and the economy stops adding up.
    #[test]
    fn the_shipped_scripts_run_every_rulers_economy() {
        let dir = mods_dir(
            "economy",
            &[
                ("base/data.ron", ECON),
                ("base/data.state.ron", ECON_STATE),
                // The real base scripts, not copies of them — so this fails if
                // either file and this surface ever drift apart.
                (
                    "base/character_levy.rhai",
                    include_str!("../../mods/base/character_levy.rhai"),
                ),
                (
                    "base/character_gold.rhai",
                    include_str!("../../mods/base/character_gold.rhai"),
                ),
            ],
        );
        let mods = load(&dir).unwrap();
        let mut ctx = Ctx::new_game(1, mods.content, mods.state);
        let mut scripts = mods.scripts;

        // Starting gold comes from the data; nothing has run yet.
        assert_eq!(purse(&ctx, "char-tywin"), (0, 0));
        assert_eq!(purse(&ctx, "char-jon"), (100, 0));

        // `on_startup` sums the holdings without paying anyone: the opening
        // screen shows the levy and the income, and the treasuries are untouched.
        scripts.run_startup(&mut ctx);
        assert_eq!(purse(&ctx, "char-tywin"), (0, 50));
        assert_eq!(gold_yield(&ctx, "char-tywin"), 1, "6 profit less 5 upkeep");
        assert_eq!(
            purse(&ctx, "char-jon"),
            (100, 0),
            "no barracks, no taxes yet"
        );
        assert_eq!(gold_yield(&ctx, "char-jon"), 10, "nothing to keep up");
        assert_eq!(
            gold_yield(&ctx, "char-hoster"),
            -5,
            "upkeep and no earnings"
        );
        assert_eq!(gold_yield(&ctx, "char-lysa"), 0, "landless earns nothing");

        day(&mut ctx, &mut scripts);
        assert_eq!(
            purse(&ctx, "char-tywin").1,
            50,
            "levy holds on the first day"
        );
        assert_eq!(purse(&ctx, "char-jon").1, 0, "a realm with no barracks");
        assert_eq!(purse(&ctx, "char-tywin").0, 0, "no taxes until the 1st");

        // Day 1 of month 2 is tick 30 on the default 30-day calendar.
        for _ in 1..30 {
            day(&mut ctx, &mut scripts);
        }
        assert_eq!((ctx.date.month, ctx.date.day), (2, 1));
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

        // Only the player's finances are worth chronicling — hoster's deficit
        // is real but goes unreported.
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
            day(&mut ctx, &mut scripts);
        }
        assert_eq!(purse(&ctx, "char-tywin"), (2, 50));
        assert_eq!(purse(&ctx, "char-jon"), (120, 0));
        assert_eq!(purse(&ctx, "char-hoster"), (-7, 50), "debt keeps deepening");
    }

    #[test]
    fn script_randomness_replays_from_the_seed() {
        let dir = mods_dir(
            "rand",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/mod.rhai",
                    r#"fn on_day(ctx) { if ctx.rand() < 0.5 { ctx.add_chronicle("heads " + ctx.tick); } }"#,
                ),
            ],
        );
        let (a, draws_a) = play(&dir, 50);
        let (b, draws_b) = play(&dir, 50);
        assert_eq!(a, b);
        assert_eq!(draws_a, draws_b);
        assert_eq!(
            draws_a, 50,
            "every script draw goes through SimRng's counter"
        );
        assert!(!a.is_empty() && a.len() < 50);
    }
}

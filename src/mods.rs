//! Mods. Each folder under the mods directory contributes data files and an
//! optional `mod.rhai`; folders load in sorted name order and later ones win.
//!
//! Every `*.ron` in a folder is a [`MapFile`], whatever it's called — the loader
//! doesn't know that `buildings.ron` holds buildings. That's what lets the base
//! game split its data by section for free, and lets a mod ship one file or six.

use crate::ctx::Ctx;
use crate::map::{self, Map, Yield};
use crate::rng::SimRng;
use anyhow::{Context, Result};
use rand::RngExt;
use rhai::{AST, Array, Dynamic, Engine, ImmutableString, Scope};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// ponytail: a runaway `while true` in a mod would wedge the render thread with
/// no way out. Cheap seatbelt; raise it if a real mod ever needs the room.
const MAX_OPS: u64 = 100_000;

/// How deeply a script may nest expressions, at top level and inside a `fn`.
///
/// Pinned because Rhai's own defaults are *halved* in debug builds (32/16
/// rather than 64/32), so an unpinned limit means a mod that runs under
/// `make play` fails under `make run`. A mod must not care which profile the
/// game was built with. These are Rhai's release numbers; raise both if a real
/// script needs the room.
const MAX_EXPR_DEPTH: usize = 64;
const MAX_FN_EXPR_DEPTH: usize = 32;

pub struct Mods {
    pub map: Map,
    pub scripts: Scripts,
}

/// Load every mod in `dir`. Bad *data* is fatal — there's no sensible game
/// without a map. Bad *scripts* are not; see [`Scripts::add`].
pub fn load(dir: &Path) -> Result<Mods> {
    let mut map = Map::default();
    let mut scripts = Scripts::new();

    for folder in sorted_entries(dir)? {
        if !folder.is_dir() {
            continue;
        }
        let name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for file in sorted_entries(&folder)? {
            let Some(ext) = file.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let read = || {
                std::fs::read_to_string(&file)
                    .with_context(|| format!("reading {}", file.display()))
            };
            match ext {
                "ron" => {
                    let text = read()?;
                    let parsed = map::parse_file(&text)
                        .with_context(|| format!("parsing {}", file.display()))?;
                    map.merge(parsed);
                }
                "rhai" => scripts.add(&name, &read()?),
                _ => {}
            }
        }
    }

    map::validate(&map).context("the merged mod data is inconsistent")?;
    Ok(Mods { map, scripts })
}

/// Directory contents, sorted by path. Sorted because load order decides who
/// overrides whom *and* how many RNG draws happen — both have to be
/// reproducible or saves and replays drift.
fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    paths.sort();
    Ok(paths)
}

/// Something a script asked the simulation to do. Collected while the hooks
/// run and applied afterwards, in order, so a script never holds a borrow on
/// `Ctx`. The `String` is the character the effect lands on.
enum Effect {
    Chronicle(String),
    AddGold(String, i64),
    SetLevy(String, u64),
}

/// Everything scripts may read about one character this tick.
#[derive(Clone, Copy, Default)]
struct CharView {
    gold: i64,
    levy: u64,
    /// What their realm yields. All zeroes unless they lead a kingdom.
    realm: Yield,
}

/// The tick's character state, built once in `Scripts::run` and shared by `Arc`
/// so cloning a `ScriptCtx` per mod per hook is a refcount bump.
///
/// ponytail: rebuilt every tick rather than kept in sync. A few dozen
/// characters is nothing next to a frame; revisit if a mod ships thousands.
#[derive(Default)]
struct Roster {
    /// Character ids in map order, so scripts iterate deterministically.
    ids: Vec<String>,
    by_id: HashMap<String, CharView>,
}

impl Roster {
    fn build(ctx: &Ctx) -> Self {
        let mut roster = Roster::default();
        for c in &ctx.map.characters {
            roster.ids.push(c.id.clone());
            roster.by_id.insert(
                c.id.clone(),
                CharView {
                    gold: c.gold,
                    levy: c.levy,
                    realm: ctx.yield_for(&c.id),
                },
            );
        }
        roster
    }

    fn get(&self, id: &str) -> CharView {
        self.by_id.get(id).copied().unwrap_or_default()
    }
}

/// What a script sees: a copy of the day's read-only state, plus handles to the
/// seeded RNG and the effect list.
///
/// Copied rather than borrowed because Rhai values must be `'static`. The
/// readable fields are a snapshot taken before any hook ran this tick — they do
/// not move as effects accumulate.
#[derive(Clone)]
pub struct ScriptCtx {
    year: i64,
    month: i64,
    day: i64,
    tick: i64,
    /// The currently selected land's id, or "" if nothing is selected.
    land: String,
    /// The character the player is playing as, for scripts that want to treat
    /// them differently — chronicle their taxes and no one else's, say.
    player: String,
    roster: Arc<Roster>,
    rng: Arc<Mutex<SimRng>>,
    out: Arc<Mutex<Vec<Effect>>>,
}

impl ScriptCtx {
    /// Uniform in `[0, 1)`, drawn from the game's seeded RNG so that a mod
    /// using randomness still replays exactly from its seed.
    fn rand(&mut self) -> f64 {
        self.rng.lock().unwrap().random::<f64>()
    }

    fn push(&mut self, effect: Effect) {
        self.out.lock().unwrap().push(effect);
    }

    fn chronicle(&mut self, line: &str) {
        self.push(Effect::Chronicle(line.to_string()));
    }

    fn add_gold(&mut self, character_id: &str, amount: i64) {
        self.push(Effect::AddGold(character_id.to_string(), amount));
    }

    /// Negative levy is meaningless, so it floors at zero rather than wrapping
    /// the `u64` on the way in.
    fn set_levy(&mut self, character_id: &str, troops: i64) {
        self.push(Effect::SetLevy(
            character_id.to_string(),
            troops.max(0) as u64,
        ));
    }
}

struct ModScript {
    name: String,
    ast: AST,
}

pub struct Scripts {
    engine: Engine,
    mods: Vec<ModScript>,
    /// Everything the scripts asked for this tick, plus any complaint about a
    /// broken mod. One channel, drained into `Ctx` at the end of `run`.
    out: Arc<Mutex<Vec<Effect>>>,
}

impl Default for Scripts {
    fn default() -> Self {
        Scripts::new()
    }
}

impl Scripts {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPS);
        engine.set_max_expr_depths(MAX_EXPR_DEPTH, MAX_FN_EXPR_DEPTH);
        engine
            .register_type_with_name::<ScriptCtx>("Ctx")
            .register_get("year", |c: &mut ScriptCtx| c.year)
            .register_get("month", |c: &mut ScriptCtx| c.month)
            .register_get("day", |c: &mut ScriptCtx| c.day)
            .register_get("tick", |c: &mut ScriptCtx| c.tick)
            .register_get("land", |c: &mut ScriptCtx| c.land.clone())
            .register_get("player", |c: &mut ScriptCtx| c.player.clone())
            .register_get("characters", |c: &mut ScriptCtx| {
                c.roster
                    .ids
                    .iter()
                    .map(|id| Dynamic::from(ImmutableString::from(id.as_str())))
                    .collect::<Array>()
            })
            // Per-character reads. An unknown id reads as all zeroes rather
            // than erroring — a script looping the roster can't hit one.
            .register_fn("gold", |c: &mut ScriptCtx, id: ImmutableString| {
                c.roster.get(&id).gold
            })
            .register_fn("levy", |c: &mut ScriptCtx, id: ImmutableString| {
                c.roster.get(&id).levy as i64
            })
            .register_fn("levy_total", |c: &mut ScriptCtx, id: ImmutableString| {
                c.roster.get(&id).realm.levy as i64
            })
            .register_fn("gold_profit", |c: &mut ScriptCtx, id: ImmutableString| {
                c.roster.get(&id).realm.gold_profit as i64
            })
            .register_fn("gold_upkeep", |c: &mut ScriptCtx, id: ImmutableString| {
                c.roster.get(&id).realm.gold_upkeep as i64
            })
            .register_fn("rand", |c: &mut ScriptCtx| c.rand())
            .register_fn(
                "add_gold",
                |c: &mut ScriptCtx, id: ImmutableString, n: i64| c.add_gold(&id, n),
            )
            .register_fn(
                "set_levy",
                |c: &mut ScriptCtx, id: ImmutableString, n: i64| c.set_levy(&id, n),
            )
            .register_fn("chronicle", |c: &mut ScriptCtx, line: ImmutableString| {
                c.chronicle(&line)
            });
        Scripts {
            engine,
            mods: Vec::new(),
            out: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Compile a mod's script. One that won't compile is reported and skipped —
    /// a broken mod should be visible and survivable, not fatal.
    pub fn add(&mut self, name: &str, source: &str) {
        match self.engine.compile(source) {
            Ok(ast) => self.mods.push(ModScript {
                name: name.into(),
                ast,
            }),
            Err(e) => self.fail(name, &e.to_string()),
        }
    }

    fn fail(&mut self, name: &str, msg: &str) {
        eprintln!("mod `{name}`: {msg}");
        self.out
            .lock()
            .unwrap()
            .push(Effect::Chronicle(format!("mod `{name}` failed: {msg}")));
    }

    /// Run this tick's hooks, then fold whatever the scripts wrote into the
    /// chronicle. A mod that throws is dropped for the rest of the session, so
    /// one bad script doesn't spam a line every single day.
    pub fn run(&mut self, ctx: &mut Ctx) {
        let sctx = ScriptCtx {
            year: i64::from(ctx.date.year),
            month: i64::from(ctx.date.month),
            day: i64::from(ctx.date.day),
            tick: ctx.tick_count as i64,
            land: ctx.selected_region.clone().unwrap_or_default(),
            player: ctx.player_character_id.clone(),
            roster: Arc::new(Roster::build(ctx)),
            rng: ctx.rng.clone(),
            out: self.out.clone(),
        };

        let mut hooks = vec!["on_day"];
        if ctx.date.is_month_start() {
            hooks.push("on_month");
        }

        let mut broken = Vec::new();
        for (i, m) in self.mods.iter().enumerate() {
            for hook in &hooks {
                // Checking the AST beats calling and swallowing a not-found
                // error: a mod defines either hook, both, or neither.
                if !m
                    .ast
                    .iter_functions()
                    .any(|f| f.name == *hook && f.params.len() == 1)
                {
                    continue;
                }
                let mut scope = Scope::new();
                let call = self
                    .engine
                    .call_fn::<()>(&mut scope, &m.ast, hook, (sctx.clone(),));
                if let Err(e) = call {
                    broken.push((i, m.name.clone(), e.to_string()));
                    break;
                }
            }
        }
        for (_, name, msg) in &broken {
            self.fail(name, msg);
        }
        // Back to front so the earlier indices stay valid.
        for (i, _, _) in broken.iter().rev() {
            self.mods.remove(*i);
        }

        // An effect naming a character the map doesn't have is dropped, not an
        // error: a mod may legitimately be written against a bigger roster.
        for effect in self.out.lock().unwrap().drain(..) {
            match effect {
                Effect::Chronicle(line) => ctx.chronicles.push(line),
                Effect::AddGold(id, n) => {
                    if let Some(c) = ctx.map.character_mut(&id) {
                        c.gold = c.gold.saturating_add(n);
                    }
                }
                Effect::SetLevy(id, n) => {
                    if let Some(c) = ctx.map.character_mut(&id) {
                        c.levy = n;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mods directory under a fresh temp dir. `files` is
    /// `(relative path, contents)`.
    fn mods_dir(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("kings-mods-{tag}"));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        root
    }

    const WORLD: &str = "(border: (x0: 0, y0: 0, x1: 10, y1: 10))";
    const LAND_1: &str = r#"(lands: [(id: "land-1", name: "first", holding: (1, 1),
        borders: [(1, 1), (2, 2)], building_ids: ["b-mill"])])"#;
    const MILL: &str = r#"(buildings: [(id: "b-mill", name: "mill", gold_profit: 6)])"#;

    #[test]
    fn later_mods_override_by_id_and_append() {
        let dir = mods_dir(
            "override",
            &[
                ("base/world.ron", WORLD),
                ("base/lands.ron", LAND_1),
                ("base/buildings.ron", MILL),
                (
                    "zz-bigger/lands.ron",
                    r#"(lands: [
                        (id: "land-1", name: "renamed", holding: (3, 3), borders: [(3, 3), (4, 4)]),
                        (id: "land-2", name: "added", holding: (5, 5), borders: [(5, 5), (6, 6)]),
                    ])"#,
                ),
            ],
        );
        let map = load(&dir).unwrap().map;
        assert_eq!(
            map.lands.len(),
            2,
            "land-1 replaced in place, land-2 appended"
        );
        assert_eq!(map.lands[0].id, "land-1");
        assert_eq!(map.lands[0].name, "renamed");
        assert_eq!(map.lands[1].name, "added");
        // The override kept its position, so ordering is stable across mods.
        assert_eq!(map.border.x1, 10.0);
    }

    #[test]
    fn splitting_a_mod_across_files_changes_nothing() {
        let split = load(&mods_dir(
            "split",
            &[
                ("base/a-world.ron", WORLD),
                ("base/b-buildings.ron", MILL),
                ("base/c-lands.ron", LAND_1),
            ],
        ))
        .unwrap()
        .map;
        let whole = load(&mods_dir(
            "whole",
            &[(
                "base/all.ron",
                r#"(border: (x0: 0, y0: 0, x1: 10, y1: 10),
                    buildings: [(id: "b-mill", name: "mill", gold_profit: 6)],
                    lands: [(id: "land-1", name: "first", holding: (1, 1),
                             borders: [(1, 1), (2, 2)], building_ids: ["b-mill"])])"#,
            )],
        ))
        .unwrap()
        .map;
        assert_eq!(format!("{split:?}"), format!("{whole:?}"));
    }

    #[test]
    fn a_mod_can_replace_the_calendar() {
        // No calendar.ron anywhere, so the default 30/12 stands.
        let plain = load(&mods_dir("cal-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .map;
        assert_eq!(plain.calendar.days_per_year(), 360);

        // A mod that ships nothing but a calendar still overrides it.
        let dir = mods_dir(
            "cal-short",
            &[
                ("base/world.ron", WORLD),
                (
                    "z-short-year/calendar.ron",
                    "(calendar: (days_per_month: 10, months_per_year: 5))",
                ),
            ],
        );
        let map = load(&dir).unwrap();
        assert_eq!(map.map.calendar.days_per_year(), 50);

        // ...and the sim actually runs on it.
        let mut ctx = Ctx::new_game(1, map.map);
        for _ in 0..50 {
            ctx.tick();
        }
        assert_eq!(ctx.date.year, 1067);
        assert_eq!((ctx.date.month, ctx.date.day), (1, 1));

        // A calendar that would never roll over is refused at load.
        let broken = mods_dir(
            "cal-zero",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/calendar.ron",
                    "(calendar: (days_per_month: 0, months_per_year: 12))",
                ),
            ],
        );
        assert!(load(&broken).is_err());
    }

    /// Two rulers and a landless character.
    ///
    /// - `char-tywin` (the player) holds `l1`: 50 levy, 6 profit, 5 upkeep.
    /// - `char-jon` holds `l2`: 0 levy, 10 profit, and starts on 100 gold.
    /// - `char-lysa` leads nothing at all.
    const ECON: &str = r#"(
        border: (x0: 0, y0: 0, x1: 10, y1: 10),
        buildings: [
            (id: "b-barracks", name: "barracks", gold_upkeep: 5, levy: 50),
            (id: "b-mill", name: "mill", gold_profit: 6),
            (id: "b-market", name: "market", gold_profit: 10),
        ],
        lands: [
            (id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)],
             building_ids: ["b-barracks", "b-mill"]),
            (id: "l2", name: "L2", holding: (5, 5), borders: [(5, 5), (6, 6)],
             building_ids: ["b-market"]),
        ],
        houses: [(id: "h1", name: "H1")],
        characters: [
            (id: "char-tywin", name: "tywin", house_id: "h1", age: 57),
            (id: "char-jon",   name: "jon",   house_id: "h1", age: 66, gold: 100),
            (id: "char-lysa",  name: "lysa",  house_id: "h1", age: 32),
        ],
        kingdoms: [
            (id: "k1", leader_character_id: "char-tywin", seat_land_id: "l1", land_ids: ["l1"]),
            (id: "k2", leader_character_id: "char-jon",   seat_land_id: "l2", land_ids: ["l2"]),
        ],
    )"#;

    /// A character's `(gold, levy)`.
    fn purse(ctx: &Ctx, id: &str) -> (i64, u64) {
        let c = ctx.map.character(id).unwrap();
        (c.gold, c.levy)
    }

    #[test]
    fn the_shipped_scripts_run_every_rulers_economy() {
        // The real base script, not a copy of it — so this fails if that file
        // and this surface ever drift apart.
        let dir = mods_dir(
            "economy",
            &[
                ("base/data.ron", ECON),
                ("base/mod.rhai", include_str!("../mods/base/mod.rhai")),
            ],
        );
        let mods = load(&dir).unwrap();
        let mut ctx = Ctx::new_game(1, mods.map);
        let mut scripts = mods.scripts;

        // Starting gold comes from the data; nothing has run yet.
        assert_eq!(purse(&ctx, "char-tywin"), (0, 0));
        assert_eq!(purse(&ctx, "char-jon"), (100, 0));

        day(&mut ctx, &mut scripts);
        assert_eq!(purse(&ctx, "char-tywin").1, 50, "levy set on the first day");
        assert_eq!(purse(&ctx, "char-jon").1, 0, "a realm with no barracks");
        assert_eq!(purse(&ctx, "char-tywin").0, 0, "no taxes until the 1st");

        // Day 1 of month 2 is tick 30 on the default 30-day calendar.
        for _ in 1..30 {
            day(&mut ctx, &mut scripts);
        }
        assert_eq!((ctx.date.month, ctx.date.day), (2, 1));
        assert_eq!(
            purse(&ctx, "char-tywin"),
            (6, 50),
            "profit only — upkeep is not deducted"
        );
        assert_eq!(
            purse(&ctx, "char-jon"),
            (110, 0),
            "every ruler collects, not just the player"
        );
        assert_eq!(
            purse(&ctx, "char-lysa"),
            (0, 0),
            "leading no kingdom earns and raises nothing"
        );

        // Only the player's taxes are worth chronicling.
        assert_eq!(
            ctx.chronicles
                .iter()
                .filter(|l| l.contains("gold in taxes"))
                .count(),
            1
        );
        assert!(ctx.chronicles.iter().any(|l| l.contains("6 gold in taxes")));

        // A second month, a second payment, and the levies hold steady.
        for _ in 0..30 {
            day(&mut ctx, &mut scripts);
        }
        assert_eq!(purse(&ctx, "char-tywin"), (12, 50));
        assert_eq!(purse(&ctx, "char-jon"), (120, 0));
    }

    #[test]
    fn a_mod_can_replace_the_speed_list() {
        let plain = load(&mods_dir("spd-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .map;
        assert_eq!(plain.speeds, vec![8, 16, 32, 64]);

        // Replaced wholesale, not appended to.
        let map = load(&mods_dir(
            "spd-slow",
            &[
                ("base/world.ron", WORLD),
                ("z-slow/calendar.ron", "(speeds: [1, 2])"),
            ],
        ))
        .unwrap()
        .map;
        assert_eq!(map.speeds, vec![1, 2]);

        // An empty list leaves nothing to run at, and a zero would stop the
        // clock dead — both are refused at load rather than at the keyboard.
        for bad in ["(speeds: [])", "(speeds: [8, 0])"] {
            let dir = mods_dir("spd-bad", &[("base/world.ron", WORLD), ("base/s.ron", bad)]);
            assert!(load(&dir).is_err(), "{bad} must not be accepted");
        }
    }

    #[test]
    fn a_mod_may_reference_another_mods_building() {
        // land-1 needs `b-mill`, which only the *second* folder declares. This
        // only works because validation waits until after the merge.
        let dir = mods_dir(
            "cross",
            &[
                ("a-lands/world.ron", WORLD),
                ("a-lands/lands.ron", LAND_1),
                ("b-buildings/buildings.ron", MILL),
            ],
        );
        assert!(load(&dir).is_ok());

        // ...and a reference nothing ever declares is still an error.
        let orphan = mods_dir(
            "orphan",
            &[("base/world.ron", WORLD), ("base/lands.ron", LAND_1)],
        );
        assert!(load(&orphan).is_err());
    }

    #[test]
    fn unknown_sections_are_rejected() {
        let dir = mods_dir(
            "typo",
            &[("base/world.ron", WORLD), ("base/x.ron", "(buildingz: [])")],
        );
        assert!(
            load(&dir).is_err(),
            "a typo'd section must not pass silently"
        );
    }

    /// One simulated day and the hooks for it.
    fn day(ctx: &mut Ctx, scripts: &mut Scripts) {
        ctx.tick();
        scripts.run(ctx);
    }

    /// Run `days` ticks over a mods dir and return the resulting chronicle,
    /// minus the opening line `Ctx::new_game` writes.
    fn play(dir: &Path, days: u32) -> (Vec<String>, u64) {
        let mods = load(dir).unwrap();
        let mut ctx = Ctx::new_game(7, mods.map);
        let mut scripts = mods.scripts;
        for _ in 0..days {
            ctx.tick();
            scripts.run(&mut ctx);
        }
        let draws = ctx.rng.lock().unwrap().draws;
        (ctx.chronicles.split_off(1), draws)
    }

    #[test]
    fn hooks_fire_daily_and_monthly() {
        let dir = mods_dir(
            "hooks",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/mod.rhai",
                    r#"
                    fn on_day(ctx) { ctx.chronicle("day " + ctx.day); }
                    fn on_month(ctx) { ctx.chronicle("month " + ctx.month); }
                    "#,
                ),
            ],
        );
        let (lines, _) = play(&dir, 31);
        // 31 days of `on_day`, plus `on_month` on the one day-1 in that span.
        assert_eq!(lines.len(), 32);
        assert_eq!(lines[0], "day 2");
        assert!(lines.contains(&"month 2".to_string()));
        assert_eq!(lines.iter().filter(|l| l.starts_with("month")).count(), 1);
    }

    #[test]
    fn a_broken_mod_is_reported_and_the_rest_keep_running() {
        let dir = mods_dir(
            "broken",
            &[
                ("base/world.ron", WORLD),
                ("a-bad/mod.rhai", "fn on_day(ctx) { this is not rhai"),
                ("b-throws/mod.rhai", r#"fn on_day(ctx) { throw "nope"; }"#),
                (
                    "c-good/mod.rhai",
                    r#"fn on_day(ctx) { ctx.chronicle("still here"); }"#,
                ),
            ],
        );
        let (lines, _) = play(&dir, 3);
        assert!(lines.iter().any(|l| l.contains("`a-bad` failed")));
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("`b-throws` failed"))
                .count(),
            1,
            "a throwing mod is dropped, not re-reported every day"
        );
        assert_eq!(lines.iter().filter(|l| *l == "still here").count(), 3);
    }

    #[test]
    fn script_randomness_replays_from_the_seed() {
        let dir = mods_dir(
            "rand",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/mod.rhai",
                    r#"fn on_day(ctx) { if ctx.rand() < 0.5 { ctx.chronicle("heads " + ctx.tick); } }"#,
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

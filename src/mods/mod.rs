//! Mods. Each folder under the mods directory contributes data files and any
//! number of scripts; folders load in sorted name order and later ones win.
//!
//! Every `*.ron` in a folder is a [`ContentFile`], whatever it's called — the
//! loader doesn't know that `buildings.ron` holds buildings. Every `*.rhai` is
//! a script, whatever it's called, and each gets its own `AST`. So a mod splits
//! itself across files however reads best (the base game names each script
//! after what it does) and the loader neither knows nor cares.
//!
//! What a hook is handed is [`script_ctx`]; what it may read and call off it is
//! [`register`], one file so that the whole mod surface is one file to read;
//! what it wrote lands in [`effects`]. Which hooks fire, and calling them, is
//! [`hooks`]; the frozen world they read is [`view`]. This one is the loader and
//! the bookkeeping.

mod effects;
mod hooks;
mod register;
mod script_ctx;
#[cfg(test)]
mod testkit;
mod view;

use crate::content::{self, Content};
use crate::ctx::Ctx;
use anyhow::{Context, Result};
use effects::Effect;
use rhai::{AST, Engine};
use script_ctx::ScriptCtx;
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
    pub content: Content,
    pub scripts: Scripts,
}

/// Load every mod in `dir`. Bad *data* is fatal — there's no sensible game
/// without content. Bad *scripts* are not; see [`Scripts::add`].
pub fn load(dir: &Path) -> Result<Mods> {
    let mut content = Content::default();
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
                    let parsed = content::parse_file(&text)
                        .with_context(|| format!("parsing {}", file.display()))?;
                    content.merge(parsed);
                }
                // Labelled `folder/file` so an error names the script that
                // broke, not just the mod it came from.
                "rhai" => {
                    let stem = file.file_stem().unwrap_or_default().to_string_lossy();
                    scripts.add(&format!("{name}/{stem}"), &read()?)
                }
                _ => {}
            }
        }
    }

    content::validate(&content).context("the merged mod data is inconsistent")?;
    Ok(Mods { content, scripts })
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
        register::script_ctx(&mut engine);
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
            .push(Effect::AddChronicle(format!("mod `{name}` failed: {msg}")));
    }

    /// Run this tick's hooks.
    pub fn run(&mut self, ctx: &mut Ctx) {
        let due = hooks::due(&ctx.date);
        self.call(ctx, &due);
    }

    /// Run `on_startup`, once, before the first tick. Whatever a script would
    /// otherwise only get right on day 1 — a ruler's levy, their monthly
    /// income — is already right on the opening screen.
    pub fn run_startup(&mut self, ctx: &mut Ctx) {
        self.call(ctx, &hooks::STARTUP);
    }

    /// Call `names` on every mod, then fold whatever the scripts wrote into the
    /// chronicle. A mod that throws is dropped for the rest of the session, so
    /// one bad script doesn't spam a line every single day.
    fn call(&mut self, ctx: &mut Ctx, names: &[&str]) {
        let sctx = ScriptCtx::build(ctx, self.out.clone());

        let broken = hooks::call(&self.engine, &self.mods, &sctx, names);
        for (_, name, msg) in &broken {
            self.fail(name, msg);
        }
        // Back to front so the earlier indices stay valid.
        for (i, _, _) in broken.iter().rev() {
            self.mods.remove(*i);
        }

        effects::drain(&self.out, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

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
        let map = load(&dir).unwrap().content;
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
        .content;
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
        .content;
        assert_eq!(format!("{split:?}"), format!("{whole:?}"));
    }

    #[test]
    fn a_mod_can_replace_the_calendar() {
        // No calendar.ron anywhere, so the default 30/12 stands.
        let plain = load(&mods_dir("cal-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .content;
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
        assert_eq!(map.content.calendar.days_per_year(), 50);

        // ...and the sim actually runs on it.
        let mut ctx = Ctx::new_game(1, map.content);
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

    #[test]
    fn a_mod_can_replace_the_speed_list() {
        let plain = load(&mods_dir("spd-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .content;
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
        .content;
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

    /// Both halves of surviving a bad mod: `add` reports one that won't compile,
    /// `run` retires one that throws.
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
                    r#"fn on_day(ctx) { ctx.add_chronicle("still here"); }"#,
                ),
            ],
        );
        let (lines, _) = play(&dir, 3);
        // Errors name the script, not just the mod it came from.
        assert!(lines.iter().any(|l| l.contains("`a-bad/mod` failed")));
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("`b-throws/mod` failed"))
                .count(),
            1,
            "a throwing mod is dropped, not re-reported every day"
        );
        assert_eq!(lines.iter().filter(|l| *l == "still here").count(), 3);
    }
}

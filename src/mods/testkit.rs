//! Fixtures shared by the tests in this module's files. Test-only.

use super::{Scripts, load};
use crate::ctx::Ctx;
use std::path::{Path, PathBuf};

pub(super) const WORLD: &str = "(border: (x0: 0, y0: 0, x1: 10, y1: 10))";
pub(super) const LAND_1: &str = r#"(lands: [(id: "land-1", name: "first", holding: (1, 1),
    borders: [(1, 1), (2, 2)])])"#;
pub(super) const MILL: &str = r#"(buildings: [(id: "b-mill", name: "mill", gold_profit: 6)])"#;
/// A `*.state.ron`: the mill stands in land-1.
pub(super) const LAND_1_MILL: &str = r#"(lands: [(id: "land-1", building_ids: ["b-mill"])])"#;

/// Build a mods directory under a fresh temp dir. `files` is
/// `(relative path, contents)`. `tag` must be unique across the whole suite —
/// tests share the temp dir and run in parallel.
pub(super) fn mods_dir(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kings-mods-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    root
}

/// One simulated day and the hooks for it.
pub(super) fn day(ctx: &mut Ctx, scripts: &mut Scripts) {
    ctx.tick();
    scripts.run(ctx);
}

/// Run `days` ticks over a mods dir and return the resulting chronicle, minus
/// the opening line `Ctx::new_game` writes, plus the RNG's draw count.
pub(super) fn play(dir: &Path, days: u32) -> (Vec<String>, u64) {
    let mods = load(dir).unwrap();
    let mut ctx = Ctx::new_game(7, mods.content, mods.state);
    let mut scripts = mods.scripts;
    scripts.run_startup(&mut ctx);
    for _ in 0..days {
        day(&mut ctx, &mut scripts);
    }
    let draws = ctx.rng.lock().unwrap().draws;
    (ctx.chronicles.split_off(1), draws)
}

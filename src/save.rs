//! Save/load: plain RON files you can edit by hand.

use crate::ecs::{Ctx, Date};
use crate::map::Map;
use crate::rng::SimRng;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Bump when the save format changes in a way old loaders can't handle.
const SAVE_VERSION: u32 = 1;

const SAVES_DIR: &str = "saves";
const QUICKSAVE_NAME: &str = "quicksave.save.ron";

/// Everything needed to restore a game. Plain RON — editable in any text editor.
#[derive(Serialize, Deserialize)]
pub struct Save {
    pub version: u32,
    pub seed: u64,
    pub rng_draws: u64,
    pub date: Date,
    pub tick_count: u64,
    pub player_character_id: Option<String>,
    pub selected_region: Option<String>,
    pub chronicles: Vec<String>,
    pub map: Map,
}

impl Save {
    /// Snapshot a running game.
    pub fn from_ctx(ctx: &Ctx) -> Self {
        let draws = ctx.rng.lock().unwrap().draws;
        Save {
            version: SAVE_VERSION,
            seed: ctx.seed,
            rng_draws: draws,
            date: ctx.date,
            tick_count: ctx.tick_count,
            player_character_id: ctx.player_character_id.clone(),
            selected_region: ctx.selected_region.clone(),
            chronicles: ctx.chronicles.clone(),
            map: ctx.map.clone(),
        }
    }

    /// Rebuild a Ctx from this save. The hecs world starts empty — no entities
    /// are persisted yet.
    pub fn restore(self) -> Ctx {
        let rng = SimRng::restore(self.seed, self.rng_draws);
        Ctx {
            world: hecs::World::new(),
            map: self.map,
            date: self.date,
            seed: self.seed,
            tick_count: self.tick_count,
            rng: Arc::new(Mutex::new(rng)),
            chronicles: self.chronicles,
            selected_region: self.selected_region,
            player_character_id: self.player_character_id,
        }
    }

    /// Load a save from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading save {}", path.display()))?;
        let save: Save = ron::from_str(&text)
            .with_context(|| format!("parsing save {}", path.display()))?;
        ensure!(
            save.version == SAVE_VERSION,
            "save version mismatch: file is {}, game expects {}",
            save.version,
            SAVE_VERSION
        );
        Ok(save)
    }

    /// Write this save to a file as pretty RON.
    pub fn write(&self, path: &Path) -> Result<()> {
        let body = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .context("serializing save")?;
        let text = format!(
            "// kings-game save — version {SAVE_VERSION}\n\
             // editable RON — edit at your own risk\n\
             {body}"
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, text)
            .with_context(|| format!("writing save {}", path.display()))?;
        Ok(())
    }
}

/// Quick-save to `saves/quicksave.save.ron`.
pub fn quicksave(ctx: &Ctx) -> Result<PathBuf> {
    let path = PathBuf::from(SAVES_DIR).join(QUICKSAVE_NAME);
    Save::from_ctx(ctx).write(&path)?;
    Ok(path)
}

/// Quick-load from `saves/quicksave.save.ron`.
pub fn quickload() -> Result<Save> {
    let path = PathBuf::from(SAVES_DIR).join(QUICKSAVE_NAME);
    Save::load(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_roundtrip() {
        let ctx = Ctx::new_game(42, Map::default(), Some("char-test".into()));
        let restored = Save::from_ctx(&ctx).restore();
        assert_eq!(restored.seed, 42);
        assert_eq!(restored.tick_count, 0);
        assert_eq!(restored.player_character_id.as_deref(), Some("char-test"));
        assert_eq!(restored.chronicles.len(), 1);
    }

    #[test]
    fn save_roundtrip_after_ticks() {
        let mut ctx = Ctx::new_game(7, Map::default(), None);
        for _ in 0..45 {
            ctx.tick();
        }
        let draws_before = ctx.rng.lock().unwrap().draws;
        let restored = Save::from_ctx(&ctx).restore();
        assert_eq!(restored.tick_count, 45);
        assert_eq!(restored.date, ctx.date);
        assert_eq!(restored.rng.lock().unwrap().draws, draws_before);
    }
}

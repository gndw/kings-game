//! Save/load: plain RON files you can edit by hand.
//!
//! The save stores **game state**, not **definitions**. Template data
//! (building stats, house names, character names) is reloaded from the map
//! file on every load. This means new content added to `map.ron` between
//! sessions is picked up automatically — new buildings become available,
//! new characters appear in the world at their definition age.

use crate::ecs::{Ctx, Date};
use crate::map::{Building, Character, House, Kingdom, Map, Rect, Shape};
use crate::rng::SimRng;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Bump when the save format changes in a way old loaders can't handle.
const SAVE_VERSION: u32 = 1;

const SAVES_DIR: &str = "saves";
const QUICKSAVE_NAME: &str = "quicksave.save.ron";

/// Template data loaded from the map file — never persisted to saves.
/// On load, definitions are read from `map.ron` and merged with save state.
#[derive(Clone)]
pub struct Definitions {
    pub buildings: Vec<Building>,
    pub houses: Vec<House>,
    pub characters: Vec<Character>,
}

impl Definitions {
    /// Extract definitions from a loaded Map.
    pub fn from_map(map: &Map) -> Self {
        Definitions {
            buildings: map.buildings.clone(),
            houses: map.houses.clone(),
            characters: map.characters.clone(),
        }
    }

    /// Load definitions from the map file path (resolved via KINGS_MAP or default).
    pub fn load_from_map_path() -> Result<Self> {
        let map_path =
            std::env::var("KINGS_MAP").unwrap_or_else(|_| "assets/map.ron".into());
        let map = crate::map::load(Path::new(&map_path))?;
        Ok(Self::from_map(&map))
    }
}

/// Runtime character state stored in the save. Only the fields that change
/// during play — name and house_id come from definitions.
#[derive(Serialize, Deserialize)]
pub struct CharacterState {
    pub id: String,
    pub age: u32,
}

/// A save file: game state + map geometry, no definitions.
///
/// What's stored here:
/// - Game state: seed, RNG draws, date, tick count, player, chronicle
/// - Map geometry: border, land polygons, holdings (save wins over map.ron)
/// - Instance state: building placements per land, kingdom ownership,
///   character ages
///
/// What's NOT stored (reloaded from map.ron on every load):
/// - Building templates (name, gold_profit, gold_upkeep, levy)
/// - House definitions (name)
/// - Character definitions (name, house_id)
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

    // --- Map geometry (save wins over map.ron) ---
    pub border: Rect,
    pub lands: Vec<Shape>,

    // --- Instance state ---
    pub kingdoms: Vec<Kingdom>,
    pub character_states: Vec<CharacterState>,
}

impl Save {
    /// Snapshot a running game, stripping out definition data.
    pub fn from_ctx(ctx: &Ctx) -> Self {
        let draws = ctx.rng.lock().unwrap().draws;
        let character_states = ctx
            .map
            .characters
            .iter()
            .map(|c| CharacterState {
                id: c.id.clone(),
                age: c.age,
            })
            .collect();

        Save {
            version: SAVE_VERSION,
            seed: ctx.seed,
            rng_draws: draws,
            date: ctx.date,
            tick_count: ctx.tick_count,
            player_character_id: ctx.player_character_id.clone(),
            selected_region: ctx.selected_region.clone(),
            chronicles: ctx.chronicles.clone(),
            border: ctx.map.border.clone(),
            lands: ctx.map.lands.clone(),
            kingdoms: ctx.map.kingdoms.clone(),
            character_states,
        }
    }

    /// Rebuild a Ctx from this save merged with definitions from the map file.
    ///
    /// Definitions provide: building templates, house names, character
    /// names/houses. Save provides: geometry, kingdom ownership, character
    /// ages, and all game state.
    ///
    /// Characters in the save but missing from definitions are dropped (their
    /// IDs dangle). Characters in definitions but not in the save enter at
    /// their definition age (they're new). Building IDs in lands that no
    /// longer exist in definitions are kept — they just won't resolve when
    /// stats are looked up.
    pub fn restore(self, defs: &Definitions) -> Ctx {
        let characters: Vec<Character> = defs
            .characters
            .iter()
            .map(|c| {
                // If the save has runtime state for this character, use the
                // saved age; otherwise the definition age (new character).
                let age = self
                    .character_states
                    .iter()
                    .find(|s| s.id == c.id)
                    .map(|s| s.age)
                    .unwrap_or(c.age);
                Character {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    house_id: c.house_id.clone(),
                    age,
                }
            })
            .collect();

        let map = Map {
            border: self.border,
            lands: self.lands,
            buildings: defs.buildings.clone(),
            houses: defs.houses.clone(),
            characters,
            kingdoms: self.kingdoms,
        };

        let rng = SimRng::restore(self.seed, self.rng_draws);
        Ctx {
            world: hecs::World::new(),
            map,
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
             // definitions (building/house/character templates) are NOT stored\n\
             // here — they reload from map.ron on every load.\n\
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

/// Quick-load from `saves/quicksave.save.ron`, merging with definitions from
/// the map file. Returns a fully restored Ctx ready to drop into Game.
pub fn quickload() -> Result<Ctx> {
    let defs = Definitions::load_from_map_path()?;
    let path = PathBuf::from(SAVES_DIR).join(QUICKSAVE_NAME);
    let save = Save::load(&path)?;
    Ok(save.restore(&defs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{House, Kingdom};

    /// Build a small but complete map for testing save/load.
    fn test_map() -> Map {
        Map {
            border: Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
            lands: vec![Shape {
                id: "l1".into(),
                name: "Land One".into(),
                holding: (5.0, 5.0),
                borders: vec![(0.0, 0.0), (10.0, 10.0)],
                building_ids: vec!["b1".into()],
            }],
            buildings: vec![Building {
                id: "b1".into(),
                name: "Market".into(),
                gold_profit: 10,
                gold_upkeep: 0,
                levy: 0,
            }],
            houses: vec![House {
                id: "h1".into(),
                name: "Testhouse".into(),
            }],
            characters: vec![Character {
                id: "c1".into(),
                name: "Alice".into(),
                house_id: "h1".into(),
                age: 30,
            }],
            kingdoms: vec![Kingdom {
                id: "k1".into(),
                leader_character_id: "c1".into(),
                seat_land_id: "l1".into(),
                land_ids: vec!["l1".into()],
            }],
        }
    }

    #[test]
    fn save_roundtrip_preserves_game_state() {
        let map = test_map();
        let ctx = Ctx::new_game(42, map, Some("c1".into()));
        let defs = Definitions::from_map(&ctx.map);

        let restored = Save::from_ctx(&ctx).restore(&defs);

        assert_eq!(restored.seed, 42);
        assert_eq!(restored.tick_count, 0);
        assert_eq!(restored.player_character_id.as_deref(), Some("c1"));
        assert_eq!(restored.chronicles.len(), 1);
        // Definitions reloaded: character name from defs, not save
        assert_eq!(restored.map.characters[0].name, "Alice");
        assert_eq!(restored.map.characters[0].age, 30);
    }

    #[test]
    fn save_roundtrip_after_ticks() {
        let mut ctx = Ctx::new_game(7, test_map(), None);
        for _ in 0..45 {
            ctx.tick();
        }
        let draws_before = ctx.rng.lock().unwrap().draws;
        let defs = Definitions::from_map(&ctx.map);

        let restored = Save::from_ctx(&ctx).restore(&defs);

        assert_eq!(restored.tick_count, 45);
        assert_eq!(restored.date, ctx.date);
        assert_eq!(restored.rng.lock().unwrap().draws, draws_before);
    }

    #[test]
    fn save_does_not_store_definitions() {
        let ctx = Ctx::new_game(1, test_map(), None);
        let save = Save::from_ctx(&ctx);

        // Save has character state (id + age) but no names or house_ids
        assert_eq!(save.character_states.len(), 1);
        assert_eq!(save.character_states[0].id, "c1");
        assert_eq!(save.character_states[0].age, 30);

        // Save serialises without building/house/character-name fields
        let ron = ron::ser::to_string_pretty(&save, ron::ser::PrettyConfig::default()).unwrap();
        assert!(!ron.contains("\"Alice\""), "character name leaked into save");
        assert!(!ron.contains("\"Market\""), "building name leaked into save");
        assert!(!ron.contains("\"Testhouse\""), "house name leaked into save");
    }

    #[test]
    fn new_characters_in_definitions_appear_on_load() {
        // Simulate: save was made with one character, then a second character
        // was added to the map file definitions.
        let ctx = Ctx::new_game(1, test_map(), None);
        let mut defs = Definitions::from_map(&ctx.map);
        defs.characters.push(Character {
            id: "c2".into(),
            name: "Bob".into(),
            house_id: "h1".into(),
            age: 25,
        });

        let restored = Save::from_ctx(&ctx).restore(&defs);

        // Both characters exist in the restored game
        assert_eq!(restored.map.characters.len(), 2);
        assert_eq!(restored.map.characters[0].id, "c1");
        assert_eq!(restored.map.characters[1].id, "c2");
        // New character enters at definition age
        assert_eq!(restored.map.characters[1].age, 25);
    }

    #[test]
    fn removed_characters_drop_on_load() {
        let ctx = Ctx::new_game(1, test_map(), None);
        // Definitions no longer have c1 (it was removed from map.ron)
        let defs = Definitions {
            buildings: vec![],
            houses: vec![],
            characters: vec![],
        };

        let restored = Save::from_ctx(&ctx).restore(&defs);

        // No characters survive — the save referenced c1 but definitions don't have it
        assert!(restored.map.characters.is_empty());
    }

    #[test]
    fn new_buildings_available_after_load() {
        let ctx = Ctx::new_game(1, test_map(), None);
        let mut defs = Definitions::from_map(&ctx.map);
        defs.buildings.push(Building {
            id: "b2".into(),
            name: "Barracks".into(),
            gold_profit: 0,
            gold_upkeep: 5,
            levy: 50,
        });

        let restored = Save::from_ctx(&ctx).restore(&defs);

        // Both buildings exist as definitions, but land still only has b1 placed
        assert_eq!(restored.map.buildings.len(), 2);
        assert_eq!(restored.map.lands[0].building_ids, vec!["b1"]);
    }
}

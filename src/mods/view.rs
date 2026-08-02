//! The tick's world, frozen. Scripts read these and never the live `Content` —
//! Rhai values must be `'static`, and a snapshot also means the readable state
//! can't shift under a hook as effects pile up.

use crate::content::Content;
use crate::state::State;
use std::collections::HashMap;

/// Everything scripts may read about one character this tick.
#[derive(Clone, Copy, Default)]
pub(super) struct CharacterView {
    pub(super) gold: i64,
    pub(super) levy: u64,
}

/// One building, as a script sees it: what one of them is worth.
#[derive(Clone, Copy, Default)]
pub(super) struct BuildingView {
    pub(super) gold_profit: u32,
    pub(super) gold_upkeep: u32,
    pub(super) levy: u32,
}

/// One land, as a script sees it: what stands in it.
#[derive(Default)]
pub(super) struct LandView {
    pub(super) building_ids: Vec<String>,
}

/// One realm, as a script sees it: who rules it and what it holds.
pub(super) struct KingdomView {
    pub(super) id: String,
    pub(super) leader: String,
    pub(super) land_ids: Vec<String>,
}

/// The world's structure, for scripts that add it up themselves — which realm
/// belongs to whom, what stands in each land, and what one of each building is
/// worth.
#[derive(Default)]
pub(super) struct RealmView {
    /// In data order, so scripts iterate deterministically.
    pub(super) kingdoms: Vec<KingdomView>,
    pub(super) lands: LandMap,
    pub(super) buildings: BuildingMap,
    pub(super) characters: CharacterMap,
}

impl RealmView {
    pub(super) fn build(content: &Content, state: &State) -> Self {
        RealmView {
            kingdoms: state
                .kingdoms
                .iter()
                .map(|k| KingdomView {
                    id: k.id.clone(),
                    leader: k.leader_character_id.clone(),
                    land_ids: k.land_ids.clone(),
                })
                .collect(),
            lands: LandMap::build(state),
            buildings: BuildingMap::build(content),
            characters: CharacterMap::build(state),
        }
    }

    /// ponytail: linear scans. A handful of kingdoms per world; index them if
    /// that ever stops being true.
    pub(super) fn kingdom(&self, id: &str) -> Option<&KingdomView> {
        self.kingdoms.iter().find(|k| k.id == id)
    }
}

/// Building id -> what one of it is worth. Individual values only; scripts sum
/// them (see `character_gold.rhai`).
#[derive(Default)]
pub(super) struct BuildingMap {
    by_id: HashMap<String, BuildingView>,
}

impl BuildingMap {
    pub(super) fn build(content: &Content) -> Self {
        let mut buildings = BuildingMap::default();
        for b in &content.buildings {
            buildings.by_id.insert(
                b.id.clone(),
                BuildingView {
                    gold_profit: b.gold_profit,
                    gold_upkeep: b.gold_upkeep,
                    levy: b.levy,
                },
            );
        }
        buildings
    }

    pub(super) fn get(&self, id: &str) -> BuildingView {
        self.by_id.get(id).copied().unwrap_or_default()
    }
}

/// Land id -> what stands in it.
#[derive(Default)]
pub(super) struct LandMap {
    by_id: HashMap<String, LandView>,
}

impl LandMap {
    pub(super) fn build(state: &State) -> Self {
        let mut lands = LandMap::default();
        for l in &state.lands {
            lands.by_id.insert(
                l.id.clone(),
                LandView {
                    building_ids: l.building_ids.clone(),
                },
            );
        }
        lands
    }

    pub(super) fn get(&self, id: &str) -> Option<&LandView> {
        self.by_id.get(id)
    }
}

/// The tick's character state, built once in `Scripts::run` and shared by `Arc`
/// so cloning a `ScriptCtx` per mod per hook is a refcount bump.
///
/// ponytail: rebuilt every tick rather than kept in sync. A few dozen
/// characters is nothing next to a frame; revisit if a mod ships thousands.
#[derive(Default)]
pub(super) struct CharacterMap {
    /// Character ids in map order, so scripts iterate deterministically.
    /// `state::reconcile` puts state in content order, so that's map order.
    pub(super) ids: Vec<String>,
    by_id: HashMap<String, CharacterView>,
}

impl CharacterMap {
    pub(super) fn build(state: &State) -> Self {
        let mut characters = CharacterMap::default();
        for c in &state.characters {
            characters.ids.push(c.id.clone());
            characters.by_id.insert(
                c.id.clone(),
                CharacterView {
                    gold: c.gold,
                    levy: c.levy,
                },
            );
        }
        characters
    }

    pub(super) fn get(&self, id: &str) -> CharacterView {
        self.by_id.get(id).copied().unwrap_or_default()
    }
}

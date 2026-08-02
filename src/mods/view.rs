//! The tick's world, frozen. Scripts read these and never the live `Content` —
//! Rhai values must be `'static`, and a snapshot also means the readable state
//! can't shift under a hook as effects pile up.

use crate::content::Content;
use crate::state::State;
use indexmap::IndexMap;

/// Everything scripts may read about one character this tick.
#[derive(Clone, Copy, Default)]
pub(super) struct CharacterView {
    pub(super) gold: i64,
    pub(super) levy: u64,
}

/// One building, as a script sees it: what one of it is worth.
#[derive(Clone, Copy, Default)]
pub(super) struct BuildingView {
    pub(super) gold_profit: u32,
    pub(super) gold_upkeep: u32,
    pub(super) levy: u32,
}

/// One land, as a script sees it: what stands in it.
#[derive(Clone, Default)]
pub(super) struct LandView {
    pub(super) building_ids: Vec<String>,
}

/// One realm, as a script sees it: who rules it and what it holds.
#[derive(Clone)]
pub(super) struct KingdomView {
    pub(super) leader: String,
    pub(super) land_ids: Vec<String>,
}

/// The world's structure, for scripts that add it up themselves — which realm
/// belongs to whom, what stands in each land, and what one of each building is
/// worth.
///
/// All collections are `IndexMap` — O(1) lookup by id for the register
/// functions, insertion-order iteration for deterministic scripts.
#[derive(Clone, Default)]
pub(super) struct RealmView {
    pub(super) kingdoms: IndexMap<String, KingdomView>,
    pub(super) lands: IndexMap<String, LandView>,
    pub(super) buildings: IndexMap<String, BuildingView>,
    pub(super) characters: IndexMap<String, CharacterView>,
}

impl RealmView {
    pub(super) fn build(content: &Content, state: &State) -> Self {
        RealmView {
            kingdoms: state
                .kingdoms
                .iter()
                .map(|(id, k)| {
                    (
                        id.clone(),
                        KingdomView {
                            leader: k.leader_character_id.clone(),
                            land_ids: k.land_ids.clone(),
                        },
                    )
                })
                .collect(),
            lands: state
                .lands
                .iter()
                .map(|(id, l)| {
                    (
                        id.clone(),
                        LandView {
                            building_ids: l.building_ids.clone(),
                        },
                    )
                })
                .collect(),
            buildings: content
                .buildings
                .iter()
                .map(|(id, b)| {
                    (
                        id.clone(),
                        BuildingView {
                            gold_profit: b.gold_profit,
                            gold_upkeep: b.gold_upkeep,
                            levy: b.levy,
                        },
                    )
                })
                .collect(),
            characters: state
                .characters
                .iter()
                .map(|(id, c)| {
                    (
                        id.clone(),
                        CharacterView {
                            gold: c.gold,
                            levy: c.levy,
                        },
                    )
                })
                .collect(),
        }
    }

    pub(super) fn kingdom(&self, id: &str) -> Option<&KingdomView> {
        self.kingdoms.get(id)
    }
}

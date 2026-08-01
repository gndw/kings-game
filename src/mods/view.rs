//! The tick's world, frozen. Scripts read these and never the live `Content` —
//! Rhai values must be `'static`, and a snapshot also means the readable state
//! can't shift under a hook as effects pile up.

use crate::content::{Content, Yield};
use std::collections::HashMap;

/// Everything scripts may read about one character this tick.
#[derive(Clone, Copy, Default)]
pub(super) struct CharView {
    pub(super) gold: i64,
    pub(super) levy: u64,
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
pub(super) struct Realms {
    /// In data order, so scripts iterate deterministically.
    pub(super) kingdoms: Vec<KingdomView>,
    /// Land id -> the buildings standing in it.
    pub(super) buildings: HashMap<String, Vec<String>>,
    /// Building id -> what it yields on its own.
    worth: HashMap<String, Yield>,
}

impl Realms {
    pub(super) fn build(content: &Content) -> Self {
        Realms {
            kingdoms: content
                .kingdoms
                .iter()
                .map(|k| KingdomView {
                    id: k.id.clone(),
                    leader: k.leader_character_id.clone(),
                    land_ids: k.land_ids.clone(),
                })
                .collect(),
            buildings: content
                .lands
                .iter()
                .map(|l| (l.id.clone(), l.building_ids.clone()))
                .collect(),
            worth: content
                .buildings
                .iter()
                .map(|b| {
                    (
                        b.id.clone(),
                        Yield {
                            levy: b.levy.into(),
                            gold_profit: b.gold_profit.into(),
                            gold_upkeep: b.gold_upkeep.into(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// ponytail: linear scans. A handful of kingdoms per world; index them if
    /// that ever stops being true.
    pub(super) fn kingdom(&self, id: &str) -> Option<&KingdomView> {
        self.kingdoms.iter().find(|k| k.id == id)
    }

    pub(super) fn building(&self, id: &str) -> Yield {
        self.worth.get(id).copied().unwrap_or_default()
    }
}

/// The tick's character state, built once in `Scripts::run` and shared by `Arc`
/// so cloning a `ScriptCtx` per mod per hook is a refcount bump.
///
/// ponytail: rebuilt every tick rather than kept in sync. A few dozen
/// characters is nothing next to a frame; revisit if a mod ships thousands.
#[derive(Default)]
pub(super) struct Roster {
    /// Character ids in map order, so scripts iterate deterministically.
    pub(super) ids: Vec<String>,
    by_id: HashMap<String, CharView>,
}

impl Roster {
    pub(super) fn build(content: &Content) -> Self {
        let mut roster = Roster::default();
        for c in &content.characters {
            roster.ids.push(c.id.clone());
            roster.by_id.insert(
                c.id.clone(),
                CharView {
                    gold: c.gold,
                    levy: c.levy,
                },
            );
        }
        roster
    }

    pub(super) fn get(&self, id: &str) -> CharView {
        self.by_id.get(id).copied().unwrap_or_default()
    }
}

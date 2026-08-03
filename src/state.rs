//! The mutable half of the world, overlaid onto [`Content`]: ages, treasuries,
//! levies, what stands in each land, and who rules what.
//!
//! State is an *overlay*, keyed by id. A `*.state.ron` ships only the entries
//! and fields it knows about; [`Content::merge_state`] fills the state fields
//! onto the matching content entries and leaves every definition field alone.
//! [`reconcile`] then repairs every reference that no longer resolves — the same
//! "old save against new content" resilience the split model had.
//!
//! On disk a state file is any `*.state.ron` in a mod folder; see
//! `crate::mods::load`.

use crate::content::{Character, Content, Kingdom, Land};
use serde::Deserialize;

/// The deserialization target for a `*.state.ron` file. It reuses the unified
/// [`Character`]/[`Land`]/[`Kingdom`] types: a state entry carries only its
/// state fields and the rest default, and [`Content::merge_state`] copies just
/// the state fields across so definition data is never clobbered.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
    #[serde(default)]
    pub lands: Vec<Land>,
    #[serde(default)]
    pub characters: Vec<Character>,
}

impl Content {
    /// Overlay one state file onto the merged content.
    ///
    /// - **Kingdoms** (state-only): id-replace, the same rule as [`merge`](Self::merge).
    /// - **Characters / lands**: field by field onto the matching content entry.
    ///   Definition fields (`name`, `house_id`, geometry) are never touched, so
    ///   a state entry may carry only its state fields.
    ///
    /// An id with no content entry is ignored — the content roster is the source
    /// of truth for what exists. (The old split model chronicled these as
    /// "dropped state for unknown …"; that note is dropped until saves exist.)
    pub fn merge_state(&mut self, file: StateFile) {
        for k in file.kingdoms {
            self.kingdoms.insert(k.id.clone(), k);
        }
        for c in file.characters {
            if let Some(existing) = self.characters.get_mut(&c.id) {
                existing.age = c.age;
                existing.gold = c.gold;
                existing.levy = c.levy;
                existing.gold_yield = c.gold_yield;
            }
        }
        for l in file.lands {
            if let Some(existing) = self.lands.get_mut(&l.id) {
                existing.building_ids = l.building_ids;
            }
        }
    }
}

/// Repair references now that content and state are one, and return a note for
/// everything that had to be repaired.
///
/// Never fails: state comes from a save that may predate — or outlive — the
/// mods it was written against, and refusing to load it would be the worst
/// possible answer.
///
/// Afterwards every building id on a land resolves, and every kingdom points at
/// a real leader and at least one real land with a valid seat.
pub fn reconcile(content: &mut Content) -> Vec<String> {
    use std::collections::HashSet;
    let mut notes = Vec::new();

    // Snapshot the id sets up front so the mutable loops below don't have to
    // fight the borrow checker for `content`. `known_buildings` is used in the
    // lands loop; the character/land sets are taken after it, since they alias
    // `content.lands`/`content.characters` which that loop and the kingdom loop
    // touch.
    let known_buildings: HashSet<&str> = content.buildings.0.keys().map(String::as_str).collect();

    // Lands: drop building ids the roster no longer defines.
    for (id, land) in &mut content.lands {
        land.building_ids.retain(|b| {
            let known = known_buildings.contains(b.as_str());
            if !known {
                notes.push(format!("land `{id}` drops unknown building `{b}`"));
            }
            known
        });
    }

    let known_chars: HashSet<&str> = content.characters.keys().map(String::as_str).collect();
    let known_lands: HashSet<&str> = content.lands.keys().map(String::as_str).collect();

    // Kingdoms: keep only those with a real leader and real lands.
    content.kingdoms.retain(|id, k| {
        if !known_chars.contains(k.leader_character_id.as_str()) {
            notes.push(format!(
                "dropped kingdom `{id}`: unknown leader `{}`",
                k.leader_character_id
            ));
            return false;
        }
        k.land_ids.retain(|l| {
            let known = known_lands.contains(l.as_str());
            if !known {
                notes.push(format!("kingdom `{id}` drops unknown land `{l}`"));
            }
            known
        });
        match k.land_ids.first() {
            None => {
                notes.push(format!("dropped kingdom `{id}`: no lands left"));
                false
            }
            Some(first) if !k.land_ids.contains(&k.seat_land_id) => {
                notes.push(format!(
                    "kingdom `{id}` seat `{}` is not one of its lands; moved to `{first}`",
                    k.seat_land_id
                ));
                k.seat_land_id = first.clone();
                true
            }
            _ => true,
        }
    });

    notes
}

/// One state file, parsed.
pub fn parse_file(text: &str) -> anyhow::Result<StateFile> {
    Ok(ron::from_str(text)?)
}

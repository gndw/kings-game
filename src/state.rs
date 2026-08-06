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

use crate::content::{Building, Character, Content, Kingdom};
use serde::Deserialize;

/// The deserialization target for a `*.state.ron` file. State entries reuse
/// the unified [`Character`]/[`Kingdom`] types and the instance [`Building`]
/// type; each carries only its state fields and the rest default, and
/// [`Content::merge_state`] copies just the state fields across so definition
/// data is never clobbered.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
    /// Building *instances* — what stands in the world. The same key
    /// (`buildings:`) means the catalogue in a definition file; here it is the
    /// mutable instance overlay. See [`Content::merge_state`].
    #[serde(default)]
    pub buildings: Vec<Building>,
    #[serde(default)]
    pub characters: Vec<Character>,
}

impl Content {
    /// Overlay one state file onto the merged content.
    ///
    /// - **Kingdoms** (state-only): id-replace, the same rule as [`merge`](Self::merge).
    /// - **Buildings** (instances): id-replace — a save holds the full set of
    ///   what's built.
    /// - **Characters**: field by field onto the matching content entry.
    ///   Definition fields (`name`, `house_id`) are never touched, so a state
    ///   entry may carry only its state fields.
    ///
    /// An id with no content entry is ignored — the content roster is the source
    /// of truth for what exists. (The old split model chronicled these as
    /// "dropped state for unknown …"; that note is dropped until saves exist.)
    pub fn merge_state(&mut self, file: StateFile) {
        for k in file.kingdoms {
            self.kingdoms.insert(k.id.clone(), k);
        }
        // Building instances: id-replace, like kingdoms — a save holds the full
        // set of what's built, so an overlay replaces rather than merges.
        for b in file.buildings {
            self.buildings.insert(b.id.clone(), b);
        }
        for c in file.characters {
            if let Some(existing) = self.characters.get_mut(&c.id) {
                existing.age = c.age;
                existing.gold = c.gold;
                existing.levy = c.levy;
                existing.gold_yield = c.gold_yield;
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
/// Afterwards every building instance points at a real definition and land,
/// and every kingdom points at a real leader and at least one real land with a
/// valid seat.
pub fn reconcile(content: &mut Content) -> Vec<String> {
    use std::collections::HashSet;
    let mut notes = Vec::new();

    // Snapshot the id sets up front so the mutable loops below don't have to
    // fight the borrow checker for `content`.
    let known_defs: HashSet<&str> =
        content.building_defs.0.keys().map(String::as_str).collect();
    let known_chars: HashSet<&str> = content.characters.keys().map(String::as_str).collect();
    let known_lands: HashSet<&str> = content.lands.keys().map(String::as_str).collect();

    // Building instances: drop any whose definition or land no longer exists.
    content.buildings.retain(|id, b| {
        if !known_defs.contains(b.def_id.as_str()) {
            notes.push(format!("dropped building `{id}`: unknown definition `{}`", b.def_id));
            return false;
        }
        if !known_lands.contains(b.land_id.as_str()) {
            notes.push(format!("dropped building `{id}`: unknown land `{}`", b.land_id));
            return false;
        }
        true
    });

    // Kingdoms: keep only those with a real leader and a real land; fix up
    // `seat_land_id` so it always equals the held land.
    content.kingdoms.retain(|id, k| {
        if !known_chars.contains(k.leader_character_id.as_str()) {
            notes.push(format!(
                "dropped kingdom `{id}`: unknown leader `{}`",
                k.leader_character_id
            ));
            return false;
        }
        if !known_lands.contains(k.land_id.as_str()) {
            notes.push(format!(
                "dropped kingdom `{id}`: unknown land `{}`",
                k.land_id
            ));
            return false;
        }
        if k.seat_land_id != k.land_id {
            notes.push(format!(
                "kingdom `{id}` seat `{}` is not its land; moved to `{}`",
                k.seat_land_id, k.land_id
            ));
            k.seat_land_id = k.land_id.clone();
        }
        true
    });

    notes
}

/// One state file, parsed.
pub fn parse_file(text: &str) -> anyhow::Result<StateFile> {
    Ok(ron::from_str(text)?)
}

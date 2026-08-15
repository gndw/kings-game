//! The mutable half of the world, overlaid onto `Content`: ages, treasuries,
//! levies, what stands in each land, and who rules what.
//!
//! State is an overlay keyed by id. `Content::merge_state` fills the state
//! fields onto the matching content entries and leaves every definition field
//! alone. `reconcile` then repairs every reference that no longer resolves.

use crate::content::{Building, Character, Content, Courtier, EventDeckState, Kingdom, Memory};
use serde::Deserialize;

/// The deserialization target for a `*.state.ron` file. State entries reuse
/// the unified `Character`/`Kingdom` types and the instance `Building` type;
/// each carries only its state fields and the rest default.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
    /// Building *instances* — what stands in the world. The same key
    /// (`buildings:`) means the catalogue in a definition file.
    #[serde(default)]
    pub buildings: Vec<Building>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub courtiers: Vec<Courtier>,
    #[serde(default)]
    pub memories: Vec<Memory>,
    /// Event popup state — read at startup, governs when the first popup
    /// fires. Plain struct (not Option) so state-file RON can use the
    /// `event_deck: (next_due_date: ...)` shorthand; `#[serde(default)]`
    /// makes the field optional (year 0 = "no state-supplied date" and the
    /// RNG first-offset fallback in `presenting_event::on_day` runs).
    #[serde(default)]
    pub event_deck: EventDeckState,
}

impl Content {
    /// Overlay one state file onto the merged content.
    /// - Kingdoms: id-replace.
    /// - Buildings (instances): id-replace — a save holds the full set of what's built.
    /// - Characters: field by field onto the matching entry. Definition fields are never touched.
    /// An id with no content entry is ignored.
    pub fn merge_state(&mut self, file: StateFile) {
        for k in file.kingdoms {
            self.kingdoms.insert(k.id.clone(), k);
        }
        for b in file.buildings {
            self.buildings.insert(b.id.clone(), b);
        }
        for c in file.characters {
            if let Some(existing) = self.characters.get_mut(&c.id) {
                existing.dob = c.dob;
                existing.gold = c.gold;
                existing.levy = c.levy;
                existing.gold_yield = c.gold_yield;
            }
        }
        for c in file.courtiers {
            self.courtiers.insert(c.id.clone(), c);
        }
        for m in file.memories {
            self.memories.insert(m.id.clone(), m);
        }
        self.event_deck = file.event_deck;
    }
}

/// Repair references and return a note for everything that had to be repaired.
/// Never fails — state comes from a save that may predate the mods it was written against.
pub fn reconcile(content: &mut Content) -> Vec<String> {
    use std::collections::HashSet;
    let mut notes = Vec::new();

    let known_defs: HashSet<&str> = content.building_defs.0.keys().map(String::as_str).collect();
    let known_chars: HashSet<&str> = content.characters.keys().map(String::as_str).collect();
    let known_lands: HashSet<&str> = content.lands.keys().map(String::as_str).collect();

    content.buildings.retain(|id, b| {
        if !known_defs.contains(b.def_id.as_str()) {
            notes.push(format!(
                "dropped building `{id}`: unknown definition `{}`",
                b.def_id
            ));
            return false;
        }
        if !known_lands.contains(b.land_id.as_str()) {
            notes.push(format!(
                "dropped building `{id}`: unknown land `{}`",
                b.land_id
            ));
            return false;
        }
        true
    });

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
        true
    });

    content.courtiers.retain(|id, c| {
        if !content.kingdoms.contains_key(&c.kingdom_id)
            || !known_chars.contains(c.character_id.as_str())
        {
            notes.push(format!(
                "dropped courtier `{id}`: unknown kingdom or character"
            ));
            false
        } else {
            true
        }
    });

    content.memories.retain(|id, m| {
        if !known_chars.contains(m.character_id.as_str())
            || !known_chars.contains(m.toward_character_id.as_str())
        {
            notes.push(format!(
                "dropped memory `{id}`: unknown character or toward_character"
            ));
            false
        } else {
            true
        }
    });

    notes
}

/// One state file, parsed.
pub fn parse_file(text: &str) -> anyhow::Result<StateFile> {
    Ok(ron::from_str(text)?)
}

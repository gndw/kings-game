//! The half of the world that changes: who rules what, what stands in each
//! land, and every character's own numbers. Everything here is written by the
//! sim and belongs in a save file; everything in [`crate::content`] is
//! read-only data the mods define and the sim never touches.
//!
//! State is an *overlay*, keyed by id. A mod — or, later, a save file — ships
//! only the entries it knows about, and [`reconcile`] fills in a default for
//! every id the content defines while dropping every entry that points at
//! content which is gone. That is what lets an old save load against new
//! content: anything added since keeps the starting state its mod shipped.
//!
//! On disk a state file is any `*.state.ron` in a mod folder; see
//! `crate::mods::load`.

use crate::content::Content;
use indexmap::IndexMap;
use serde::Deserialize;

/// Everything mutable, after every `*.state.ron` has been merged in.
///
/// ID-keyed (`IndexMap`) for O(1) lookup and deterministic insertion-order
/// iteration — the same rule as `Content`.
#[derive(Debug, Default)]
pub struct State {
    /// Realms. Wholly state — a kingdom's leader, seat and lands all change in
    /// play. Nothing about a kingdom is fixed data yet, so there is no
    /// definition side to join against.
    pub kingdoms: IndexMap<String, Kingdom>,
    pub lands: IndexMap<String, LandState>,
    pub characters: IndexMap<String, CharacterState>,
}

/// The deserialization target for a `*.state.ron` file. Collections are `Vec`
/// here because RON files are arrays of structs; [`State::merge`] inserts them
/// into `IndexMap`s keyed by id.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFile {
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
    #[serde(default)]
    pub lands: Vec<LandState>,
    #[serde(default)]
    pub characters: Vec<CharacterState>,
}

impl From<StateFile> for State {
    fn from(file: StateFile) -> Self {
        let mut state = State::default();
        for k in file.kingdoms {
            state.kingdoms.insert(k.id.clone(), k);
        }
        for l in file.lands {
            state.lands.insert(l.id.clone(), l);
        }
        for c in file.characters {
            state.characters.insert(c.id.clone(), c);
        }
        state
    }
}

/// A realm: a ruler, a capital, and the lands it holds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kingdom {
    pub id: String,
    pub leader_character_id: String,
    pub seat_land_id: String,
    pub land_ids: Vec<String>,
}

/// What stands in one land. The land itself — its name and its outline — is
/// content; what has been built on it is not.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandState {
    pub id: String,
    /// Ids into `Content::buildings`.
    #[serde(default)]
    pub building_ids: Vec<String>,
}

/// One character's numbers. Their name and house are content.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterState {
    pub id: String,
    #[serde(default)]
    pub age: u32,
    /// Treasury. Signed, so a script may spend past zero.
    #[serde(default)]
    pub gold: i64,
    /// Troops currently raised. Only a character who leads a kingdom has
    /// holdings to raise them from.
    #[serde(default)]
    pub levy: u64,
    /// Gold per month: what their holdings render at the next payout, profit
    /// less upkeep. Signed, like `gold` — a realm that garrisons more than it
    /// earns runs at a loss. Written by the same script that pays it, so the
    /// two can't disagree.
    ///
    /// Recomputed by `on_startup`, so a save carrying a stale one self-corrects
    /// on load.
    #[serde(default)]
    pub gold_yield: i64,
}

impl State {
    /// Fold one state into another. Same id rule as content: same id replaces,
    /// new id appends.
    pub fn merge(&mut self, other: State) {
        for (k, v) in other.kingdoms {
            self.kingdoms.insert(k, v);
        }
        for (k, v) in other.lands {
            self.lands.insert(k, v);
        }
        for (k, v) in other.characters {
            self.characters.insert(k, v);
        }
    }

    /// The kingdom holding `land_id`, if any.
    pub fn kingdom_of(&self, land_id: &str) -> Option<&Kingdom> {
        self.kingdoms
            .values()
            .find(|k| k.land_ids.iter().any(|l| l == land_id))
    }

    /// The kingdom `character_id` rules, if any.
    pub fn kingdom_led_by(&self, character_id: &str) -> Option<&Kingdom> {
        self.kingdoms
            .values()
            .find(|k| k.leader_character_id == character_id)
    }

    pub fn character(&self, id: &str) -> Option<&CharacterState> {
        self.characters.get(id)
    }

    /// For the sim to write a character's gold and levy back.
    pub fn character_mut(&mut self, id: &str) -> Option<&mut CharacterState> {
        self.characters.get_mut(id)
    }

    /// What stands in `land_id`. Empty for a land nothing has been built on.
    pub fn buildings_in(&self, land_id: &str) -> &[String] {
        self.lands
            .get(land_id)
            .map_or(&[], |l| l.building_ids.as_slice())
    }
}

/// Pull the merged state up with the merged content, and return a note for
/// everything that had to be repaired.
///
/// Unlike [`crate::content::validate`], this *never* fails. Content is
/// authored, so a broken reference there is a bug worth stopping for; state
/// comes from a save that may predate — or outlive — the mods it was written
/// against, and refusing to load it would be the worst possible answer.
///
/// Afterwards there is exactly one state entry per content land and per content
/// character, in content order, and every remaining reference resolves.
pub fn reconcile(content: &Content, state: &mut State) -> Vec<String> {
    let mut notes = Vec::new();

    // Characters: one entry per content character, in content order.
    let mut new_characters = IndexMap::new();
    for (id, _) in &content.characters {
        new_characters.insert(
            id.clone(),
            state.characters.shift_remove(id).unwrap_or_else(|| CharacterState {
                id: id.clone(),
                ..Default::default()
            }),
        );
    }
    for (id, _) in &state.characters {
        notes.push(format!("dropped state for unknown character `{id}`"));
    }
    state.characters = new_characters;

    // Lands: one entry per content land, in content order.
    let mut new_lands = IndexMap::new();
    for (id, _) in &content.lands {
        let mut land = state.lands.shift_remove(id).unwrap_or_else(|| LandState {
            id: id.clone(),
            ..Default::default()
        });
        land.building_ids.retain(|b| {
            let known = content.buildings.contains_key(b);
            if !known {
                notes.push(format!("land `{id}` drops unknown building `{b}`"));
            }
            known
        });
        new_lands.insert(id.clone(), land);
    }
    for (id, _) in &state.lands {
        notes.push(format!("dropped state for unknown land `{id}`"));
    }
    state.lands = new_lands;

    // Kingdoms: state-only (no content counterpart), so retain in place.
    state.kingdoms.retain(|id, k| {
        if !content.characters.contains_key(&k.leader_character_id) {
            notes.push(format!(
                "dropped kingdom `{id}`: unknown leader `{}`",
                k.leader_character_id
            ));
            return false;
        }
        k.land_ids.retain(|l| {
            let known = content.lands.contains_key(l);
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

/// One state file, parsed and converted to runtime [`State`]. Same shape as
/// the merged whole, because every section is already optional — a save writes
/// exactly what a mod would.
pub fn parse_file(text: &str) -> anyhow::Result<State> {
    let file: StateFile = ron::from_str(text)?;
    Ok(file.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content;

    /// Content with two characters and two lands; the state below knows about
    /// less than that, and about things that don't exist.
    const CONTENT: &str = r#"(
        border: (x0: 0, y0: 0, x1: 10, y1: 10),
        buildings: [(id: "b-mill", name: "mill", gold_profit: 6)],
        lands: [
            (id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)]),
            (id: "l2", name: "L2", holding: (5, 5), borders: [(5, 5), (6, 6)]),
        ],
        houses: [(id: "h1", name: "H1")],
        characters: [
            (id: "c1", name: "C1", house_id: "h1"),
            (id: "c2", name: "C2", house_id: "h1"),
        ],
    )"#;

    /// The whole point of the split: a save written before `l2`, `c2` and the
    /// mill existed still loads, and the content added since fills in.
    #[test]
    fn an_old_save_loads_against_new_content() {
        let content = content::parse(CONTENT).unwrap();
        let mut state = parse_file(
            r#"(
                characters: [
                    (id: "c1", age: 40, gold: 12),
                    (id: "gone", age: 99),
                ],
                lands: [
                    (id: "l1", building_ids: ["b-mill", "b-vanished"]),
                    (id: "sunk", building_ids: []),
                ],
                kingdoms: [
                    (id: "k1", leader_character_id: "c1", seat_land_id: "atlantis",
                     land_ids: ["l1", "atlantis"]),
                    (id: "k-ghost", leader_character_id: "nobody", seat_land_id: "l2",
                     land_ids: ["l2"]),
                ],
            )"#,
        )
        .unwrap();
        let notes = reconcile(&content, &mut state);

        // What the save knew survives...
        assert_eq!(state.character("c1").unwrap().gold, 12);
        assert_eq!(state.buildings_in("l1"), ["b-mill"]);
        // ...content it never heard of gets a default...
        assert_eq!(state.character("c2").unwrap().age, 0);
        assert!(state.buildings_in("l2").is_empty());
        // ...and state pointing at content that's gone is dropped, not fatal.
        assert!(state.character("gone").is_none());
        assert!(state.kingdom_led_by("nobody").is_none());
        let k = state.kingdom_of("l1").unwrap();
        assert_eq!(k.land_ids, ["l1"], "atlantis pruned");
        assert_eq!(k.seat_land_id, "l1", "seat followed its lands");
        assert_eq!(notes.len(), 6, "every repair says so: {notes:#?}");

        // One entry per definition, in content order, so scripts iterate the
        // same way every run.
        let ids: Vec<&str> = state.characters.keys().map(|s| s.as_str()).collect();
        assert_eq!(ids, ["c1", "c2"]);
        let ids: Vec<&str> = state.lands.keys().map(|s| s.as_str()).collect();
        assert_eq!(ids, ["l1", "l2"]);

        // Reconciling again is a no-op — nothing left to repair.
        assert!(reconcile(&content, &mut state).is_empty());
    }

    #[test]
    fn later_state_files_override_by_id() {
        let mut state =
            parse_file(r#"(characters: [(id: "c1", gold: 1), (id: "c2", gold: 2)])"#).unwrap();
        state.merge(
            parse_file(r#"(characters: [(id: "c2", gold: 99), (id: "c3", gold: 3)])"#).unwrap(),
        );
        assert_eq!(state.character("c1").unwrap().gold, 1);
        assert_eq!(state.character("c2").unwrap().gold, 99);
        assert_eq!(state.character("c3").unwrap().gold, 3);
    }
}

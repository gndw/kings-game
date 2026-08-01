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

use crate::content::{Content, merge_by_id};
use serde::Deserialize;

/// Everything mutable, after every `*.state.ron` has been merged in.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// Realms. Wholly state — a kingdom's leader, seat and lands all change in
    /// play. Nothing about a kingdom is fixed data yet, so there is no
    /// definition side to join against.
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
    #[serde(default)]
    pub lands: Vec<LandState>,
    #[serde(default)]
    pub characters: Vec<CharacterState>,
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
    /// Fold one file in, by the same id rule as content: same id replaces,
    /// new id appends.
    pub fn merge(&mut self, file: State) {
        merge_by_id(&mut self.kingdoms, file.kingdoms, |k| &k.id);
        merge_by_id(&mut self.lands, file.lands, |l| &l.id);
        merge_by_id(&mut self.characters, file.characters, |c| &c.id);
    }

    /// The kingdom holding `land_id`, if any.
    pub fn kingdom_of(&self, land_id: &str) -> Option<&Kingdom> {
        self.kingdoms
            .iter()
            .find(|k| k.land_ids.iter().any(|l| l == land_id))
    }

    /// The kingdom `character_id` rules, if any.
    pub fn kingdom_led_by(&self, character_id: &str) -> Option<&Kingdom> {
        self.kingdoms
            .iter()
            .find(|k| k.leader_character_id == character_id)
    }

    pub fn character(&self, id: &str) -> Option<&CharacterState> {
        self.characters.iter().find(|c| c.id == id)
    }

    /// For the sim to write a character's gold and levy back.
    pub fn character_mut(&mut self, id: &str) -> Option<&mut CharacterState> {
        self.characters.iter_mut().find(|c| c.id == id)
    }

    /// What stands in `land_id`. Empty for a land nothing has been built on.
    pub fn buildings_in(&self, land_id: &str) -> &[String] {
        self.lands
            .iter()
            .find(|l| l.id == land_id)
            .map_or(&[], |l| l.building_ids.as_slice())
    }
}

/// Pull `id` out of `v`, if it's there.
///
/// ponytail: linear scan, like `merge_by_id` next door. Index by id if a mod
/// set ever gets big enough to notice.
fn take_by_id<T>(v: &mut Vec<T>, id: &str, key: impl for<'a> Fn(&'a T) -> &'a str) -> Option<T> {
    v.iter().position(|x| key(x) == id).map(|i| v.remove(i))
}

/// Line the merged state up with the merged content, and return a note for
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

    let mut characters = Vec::with_capacity(content.characters.len());
    for c in &content.characters {
        characters.push(
            take_by_id(&mut state.characters, &c.id, |s| &s.id).unwrap_or_else(|| CharacterState {
                id: c.id.clone(),
                ..Default::default()
            }),
        );
    }
    for stale in state.characters.drain(..) {
        notes.push(format!(
            "dropped state for unknown character `{}`",
            stale.id
        ));
    }
    state.characters = characters;

    let mut lands = Vec::with_capacity(content.lands.len());
    for l in &content.lands {
        let mut land =
            take_by_id(&mut state.lands, &l.id, |s| &s.id).unwrap_or_else(|| LandState {
                id: l.id.clone(),
                ..Default::default()
            });
        land.building_ids.retain(|b| {
            let known = content.building(b).is_some();
            if !known {
                notes.push(format!("land `{}` drops unknown building `{b}`", l.id));
            }
            known
        });
        lands.push(land);
    }
    for stale in state.lands.drain(..) {
        notes.push(format!("dropped state for unknown land `{}`", stale.id));
    }
    state.lands = lands;

    state.kingdoms.retain_mut(|k| {
        let id = k.id.clone();
        if content.character(&k.leader_character_id).is_none() {
            notes.push(format!(
                "dropped kingdom `{id}`: unknown leader `{}`",
                k.leader_character_id
            ));
            return false;
        }
        k.land_ids.retain(|l| {
            let known = content.lands.iter().any(|d| &d.id == l);
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

/// One state file. Same shape as the merged whole, because every section is
/// already optional — a save writes exactly what a mod would.
pub fn parse_file(text: &str) -> anyhow::Result<State> {
    Ok(ron::from_str(text)?)
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
        let ids: Vec<&str> = state.characters.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["c1", "c2"]);
        let ids: Vec<&str> = state.lands.iter().map(|l| l.id.as_str()).collect();
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

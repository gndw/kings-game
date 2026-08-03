//! Everything the mods define, with the starting state overlaid on top: map
//! geometry, the calendar, and the houses, characters, lands and kingdoms that
//! populate the world. All of it comes from RON data files at startup so it can
//! be modded without a rebuild (see `mods/base/`).
//!
//! Loaded in two phases by [`crate::mods`]: first every definition file merges
//! in (id-replace), then every `*.state.ron` overlays the mutable half — ages,
//! treasuries, levies, what's built, who rules what — onto the *same* structs
//! field by field (see [`crate::state`]). The result is one struct per entity
//! kind, passed whole into [`crate::ecs::populate`].
//!
//! `crate::mods` does the loading and merging; the camera and drawing live in
//! `crate::ui::map`.

use crate::resources::border::Border;
use crate::resources::buildings::{Building, Buildings};
use crate::resources::calendar::Calendar;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;

/// Everything the mods define plus the starting state, after every definition
/// file has merged in and the state overlaid.
///
/// Named `Content` rather than `Map` because it long ago stopped being just
/// geometry — it also carries the calendar, the roster of characters, and who
/// rules what.
#[derive(Debug)]
pub struct Content {
    pub border: Border,
    /// How long a month and a year are. Not geometry, but it arrives the same
    /// way every other mod section does.
    pub calendar: Calendar,
    /// ID-keyed for O(1) lookup; insertion-ordered for deterministic iteration.
    pub lands: IndexMap<String, Land>,
    /// The building roster, carried through as a resource and seeded into the
    /// world in `ecs::populate`.
    pub buildings: Buildings,
    pub houses: IndexMap<String, House>,
    pub characters: IndexMap<String, Character>,
    /// Realms. Wholly state — a kingdom's leader, seat and lands all change in
    /// play — so they arrive only via the state overlay.
    pub kingdoms: IndexMap<String, Kingdom>,
}

/// Hand-written rather than derived because an empty `speeds` list is not a
/// usable game — a derived `Default` would hand out one silently.
impl Default for Content {
    fn default() -> Self {
        Content {
            border: Border::default(),
            calendar: Calendar::default(),
            lands: IndexMap::new(),
            buildings: Buildings::default(),
            houses: IndexMap::new(),
            characters: IndexMap::new(),
            kingdoms: IndexMap::new(),
        }
    }
}

/// One definition file on disk. Every section is optional, so a mod ships only
/// what it changes — and the base game can split itself across `lands.ron`,
/// `buildings.ron` and friends without the loader knowing the difference.
///
/// `deny_unknown_fields` so a modder's typo is an error instead of a section
/// that silently does nothing.
///
/// Collections stay as `Vec` here — this is a single file's contribution,
/// merged into [`Content`]'s `IndexMap`s by [`Content::merge`].
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFile {
    #[serde(default)]
    pub border: Option<Border>,
    #[serde(default)]
    pub calendar: Option<Calendar>,
    #[serde(default)]
    pub lands: Vec<Land>,
    #[serde(default)]
    pub buildings: Vec<Building>,
    #[serde(default)]
    pub houses: Vec<House>,
    #[serde(default)]
    pub characters: Vec<Character>,
}

impl Content {
    /// Fold one definition file in. An entry whose `id` already exists replaces
    /// the earlier one, anything else appends — that is the whole override rule.
    /// With `IndexMap`, `insert` does both: same key replaces in place, new key
    /// appends at the end. Insertion order is preserved across merges.
    pub fn merge(&mut self, file: ContentFile) {
        if let Some(border) = file.border {
            self.border = border;
        }
        if let Some(calendar) = file.calendar {
            self.calendar = calendar;
        }
        for land in file.lands {
            self.lands.insert(land.id.clone(), land);
        }
        for building in file.buildings {
            self.buildings.0.insert(building.id.clone(), building);
        }
        for house in file.houses {
            self.houses.insert(house.id.clone(), house);
        }
        for character in file.characters {
            self.characters.insert(character.id.clone(), character);
        }
    }
}

/// One land: its geometry (definition) plus what stands on it (state). The
/// `building_ids` arrive empty from a definition file and are filled in by the
/// state overlay.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Land {
    pub id: String,
    // Every non-id field defaults: a definition file carries the geometry
    // (name/borders/holding), a state file carries only `building_ids`, and
    // each omits the other's fields. The overlay keeps the two halves from
    // clobbering each other — see `Content::merge_state`.
    #[serde(default)]
    pub name: String,
    /// This land's own outline, a polyline of `(x, y)` points. Not to be
    /// confused with `Content::border`, the edge of the world.
    #[serde(default)]
    pub borders: Vec<(f64, f64)>,
    /// Seat of power, somewhere inside `borders`. Drawn as a circle.
    #[serde(default)]
    pub holding: (f64, f64),
    /// What has been built here — ids into `Content::buildings`. State, filled
    /// by the `*.state.ron` overlay; empty on a definition-only entry.
    #[serde(default)]
    pub building_ids: Vec<String>,
}

/// A family. Characters belong to one; kingdoms are ruled through them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct House {
    pub id: String,
    pub name: String,
}

/// One character: who they are (definition) plus their numbers (state). Age,
/// treasury, levy and yield arrive at zero from a definition file and are filled
/// in by the state overlay.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub id: String,
    // Every non-id field defaults: a definition file carries name/house_id, a
    // state file carries only the numbers, and each omits the other's fields.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub house_id: String,
    /// State: years. Defaults to 0 on a definition-only entry.
    #[serde(default)]
    pub age: u32,
    /// State: treasury. Signed, so a script may spend past zero.
    #[serde(default)]
    pub gold: i64,
    /// State: troops currently raised. Only a character who leads a kingdom has
    /// holdings to raise them from.
    #[serde(default)]
    pub levy: u64,
    /// State: gold per month — what their holdings render at the next payout,
    /// profit less upkeep. Signed, like `gold`. Recomputed by `on_startup`, so a
    /// save carrying a stale one self-corrects on load.
    #[serde(default)]
    pub gold_yield: i64,
}

/// A realm: a ruler, a capital, and the lands it holds. Wholly state — there is
/// no definition half — so a kingdom only exists once the state overlay adds it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kingdom {
    pub id: String,
    pub leader_character_id: String,
    pub seat_land_id: String,
    pub land_ids: Vec<String>,
}

impl Content {
    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.get(id)
    }
}

/// One definition file. No cross-reference checking — a mod may legitimately
/// point at a building some other mod declares, so that waits for [`validate`].
pub fn parse_file(text: &str) -> Result<ContentFile> {
    // IMPLICIT_SOME so an optional section is written `border: (...)` rather
    // than `border: Some((...))` — modders shouldn't have to know which
    // sections happen to be `Option` on the Rust side.
    let opts =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    Ok(opts.from_str(text)?)
}

/// Check the content hangs together. Runs on the *merged* result, never on one
/// file.
///
/// Fatal, unlike [`crate::state::reconcile`]: content is authored by hand, so a
/// dangling reference here is a mod bug worth stopping for.
pub fn validate(content: &Content) -> Result<()> {
    let b = &content.border;
    if b.x1 <= b.x0 || b.y1 <= b.y0 {
        bail!("map border must have x1 > x0 and y1 > y0");
    }
    content.calendar.validate()?;
    for (_, s) in &content.lands {
        if s.borders.len() < 2 {
            bail!("land `{}` needs at least 2 border points", s.id);
        }
    }
    for (_, c) in &content.characters {
        if !content.houses.contains_key(&c.house_id) {
            bail!(
                "character `{}` references unknown house `{}`",
                c.id,
                c.house_id
            );
        }
    }
    Ok(())
}

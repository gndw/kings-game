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
//! `crate::ui::input::map_selection` (root layer) plus the
//! `map::components` siblings.

use crate::ecs::{BuildingStatus, CourtierType};
use crate::resources::border::Border;
use crate::resources::buildings::{BuildingDef, BuildingDefs};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
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
    /// The read-only building-definition roster (one entry per building kind),
    /// carried through as a resource and seeded into the world in
    /// `ecs::populate`.
    pub building_defs: BuildingDefs,
    /// Building *instances* — what actually stands in the world, one per built
    /// building. State-only (a save holds what's built); spawned as entities by
    /// `ecs::populate`. Keyed by instance id.
    pub buildings: IndexMap<String, Building>,
    pub houses: IndexMap<String, House>,
    pub characters: IndexMap<String, Character>,
    /// Realms. Wholly state — a kingdom's leader, seat and lands all change in
    /// play — so they arrive only via the state overlay.
    pub kingdoms: IndexMap<String, Kingdom>,
    pub courtiers: IndexMap<String, Courtier>,
    /// Connections between adjacent lands. Definition-only — the polyline and
    /// the two lands it joins don't change in play — so a road is built once
    /// at populate time.
    pub roads: IndexMap<String, Road>,
}

/// Hand-written rather than derived because an empty `speeds` list is not a
/// usable game — a derived `Default` would hand out one silently.
impl Default for Content {
    fn default() -> Self {
        Content {
            border: Border::default(),
            calendar: Calendar::default(),
            lands: IndexMap::new(),
            building_defs: BuildingDefs::default(),
            buildings: IndexMap::new(),
            houses: IndexMap::new(),
            characters: IndexMap::new(),
            kingdoms: IndexMap::new(),
            courtiers: IndexMap::new(),
            roads: IndexMap::new(),
        }
    }
}

/// One definition file on disk. Every section is optional, so a mod ships only
/// what it changes — and the base game can split itself across `lands.ron`,
/// `building_definitions.ron` and friends without the loader knowing the
/// difference.
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
    /// The `buildings:` section of a *definition* file is the catalogue — one
    /// [`BuildingDef`] per kind. (In a `*.state.ron` the same key holds instance
    /// overlays instead; see [`crate::state::StateFile`].)
    #[serde(default)]
    pub buildings: Vec<BuildingDef>,
    #[serde(default)]
    pub houses: Vec<House>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub roads: Vec<Road>,
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
        for def in file.buildings {
            self.building_defs.0.insert(def.id.clone(), def);
        }
        for house in file.houses {
            self.houses.insert(house.id.clone(), house);
        }
        for character in file.characters {
            self.characters.insert(character.id.clone(), character);
        }
        for road in file.roads {
            self.roads.insert(road.id.clone(), road);
        }
    }
}

/// One land: pure geometry (definition). What stands on a land is no longer a
/// field here — each built building is its own entity, related to the land via
/// `ecs::BuildingOnLand` (the instances live in [`Content::buildings`]).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Land {
    pub id: String,
    // Every non-id field defaults: a definition file carries the geometry
    // (name/borders/holding), and a state file may carry only the id.
    #[serde(default)]
    pub name: String,
    /// This land's own outline, a polyline of `(x, y)` points. Not to be
    /// confused with `Content::border`, the edge of the world.
    #[serde(default)]
    pub borders: Vec<(f64, f64)>,
    /// Seat of power, somewhere inside `borders`. Drawn as a circle.
    #[serde(default)]
    pub holding: (f64, f64),
}

/// A family. Characters belong to one; kingdoms are ruled through them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct House {
    pub id: String,
    pub name: String,
}

/// One character: who they are (definition) plus their numbers (state). Date
/// of birth, treasury, levy and yield arrive at zero / the default date from a
/// definition file and are filled in by the state overlay.
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
    /// State: date of birth, in calendar terms. A character's age is derived
    /// from this against the current date — see [`crate::game::age`]. Defaults
    /// to `Date::default()` on a definition-only entry.
    #[serde(default)]
    pub dob: Date,
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

/// One built building instance: which definition it is an instance of, which
/// land it stands on, and what state it is in. State-only (what's built
/// changes in play and belongs in a save), like [`Kingdom`]. Spawned as an
/// entity by [`crate::ecs::populate`] and related to its land via
/// [`BuildingOnLand`](crate::ecs::BuildingOnLand).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    pub id: String,
    /// The building *definition* id — which kind of building this is. A key
    /// into [`Content::building_defs`].
    pub def_id: String,
    /// The land this building stands on, by id.
    pub land_id: String,
    /// Per-instance operating state. Defaults to `Active`.
    #[serde(default)]
    pub status: BuildingStatus,
}

/// A realm: a ruler and the single land it holds (which is also its capital).
/// Wholly state — there is no definition half — so a kingdom only exists once
/// the state overlay adds it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kingdom {
    pub id: String,
    pub leader_character_id: String,
    pub land_id: String,
}

/// A kingdom court appointment.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Courtier {
    pub id: String,
    pub kingdom_id: String,
    pub character_id: String,
    #[serde(rename = "type")]
    pub courtier_type: CourtierType,
}

/// A connection between two lands — a polyline (one point per segment), the
/// two land ids it joins, and how many days marching it takes.
/// Definition-only: roads are baked at populate time into
/// [`crate::ecs::road::Road`] entities and never edited.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Road {
    pub id: String,
    #[serde(default)]
    pub points: Vec<(f64, f64)>,
    /// Exactly two land ids — the two lands the road joins. Resolved to
    /// [`crate::ecs::road::RoadBetweenLands`] entities at populate time.
    #[serde(default)]
    pub between_land_ids: Vec<String>,
    /// Days an army spends marching this road, the whole cost of one
    /// marching (one marching entity covers one road). Authored, not
    /// derived from `points` — the base mod scales it off polyline length
    /// (longest road = 30 days), but a mod is free to price a road against
    /// its geometry. Resolved to
    /// [`crate::ecs::road::RoadDistanceDays`] at populate time; [`validate`]
    /// rejects 0.
    #[serde(default)]
    pub distance_days: u32,
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
    for (_, r) in &content.roads {
        if r.points.len() < 2 {
            bail!("road `{}` needs at least 2 points", r.id);
        }
        if r.between_land_ids.len() != 2 {
            bail!(
                "road `{}` needs exactly 2 land ids, got {}",
                r.id,
                r.between_land_ids.len()
            );
        }
        for lid in &r.between_land_ids {
            if !content.lands.contains_key(lid) {
                bail!("road `{}` references unknown land `{lid}`", r.id);
            }
        }
        // A free road would let an army teleport (begin and arrive the same
        // day), so a missing or zero `distance_days` is a mod bug.
        if r.distance_days == 0 {
            bail!("road `{}` needs a `distance_days` of at least 1", r.id);
        }
    }
    Ok(())
}

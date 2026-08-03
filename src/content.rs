//! Everything the mods *define*: map geometry, the calendar, and the houses,
//! characters and buildings that populate the world. All of it
//! comes from RON data files at startup so it can be modded without a rebuild
//! (see `mods/base/`).
//!
//! Read-only once loaded. Everything the sim writes — treasuries, levies, who
//! rules what, what has been built — is [`crate::state`], which is keyed by the
//! ids declared here and is what a save file would hold.
//!
//! `crate::mods` does the loading and merging; the camera and drawing live in
//! `crate::ui::map`.

use crate::resources::border::Border;
use crate::resources::buildings::{Building, Buildings};
use crate::resources::calendar::Calendar;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;

/// Everything defined, after every mod file has been merged in.
///
/// Named `Content` rather than `Map` because it long ago stopped being just
/// geometry — it also carries the calendar and the roster of characters the
/// sim's state hangs off.
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
        }
    }
}

/// One data file on disk. Every section is optional, so a mod ships only what
/// it changes — and the base game can split itself across `lands.ron`,
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
    /// Fold one file in. An entry whose `id` already exists replaces the
    /// earlier one, anything else appends — that is the whole override rule.
    /// With `IndexMap`, `insert` does both: same key replaces in place, new
    /// key appends at the end. Insertion order is preserved across merges.
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

/// One land, an entry in `lands.ron`.
#[derive(Debug, Deserialize)]
pub struct Land {
    pub id: String,
    pub name: String,
    /// This land's own outline, a polyline of `(x, y)` points. Not to be
    /// confused with `Content::border`, the edge of the world.
    pub borders: Vec<(f64, f64)>,
    /// Seat of power, somewhere inside `borders`. Drawn as a circle.
    pub holding: (f64, f64),
}

/// A family. Characters belong to one; kingdoms are ruled through them.
#[derive(Debug, Deserialize)]
pub struct House {
    pub id: String,
    pub name: String,
}

/// Who exists and where they come from. Their age, treasury and levy change in
/// play, so those live on `state::CharacterState` under the same id.
#[derive(Debug, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub house_id: String,
}

impl Content {
    pub fn building(&self, id: &str) -> Option<&Building> {
        self.buildings.get(id)
    }

    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.get(id)
    }

    pub fn house(&self, id: &str) -> Option<&House> {
        self.houses.get(id)
    }
}

/// One data file. No cross-reference checking — a mod may legitimately point at
/// a building some other mod declares, so that waits for [`validate`].
pub fn parse_file(text: &str) -> Result<ContentFile> {
    // IMPLICIT_SOME so an optional section is written `border: (...)` rather
    // than `border: Some((...))` — modders shouldn't have to know which
    // sections happen to be `Option` on the Rust side.
    let opts =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    Ok(opts.from_str(text)?)
}

/// A whole Content from a single file, for tests and one-file mods. The game merges
/// many instead, via `crate::mods::load`.
pub fn parse(text: &str) -> Result<Content> {
    let mut content = Content::default();
    content.merge(parse_file(text)?);
    validate(&content)?;
    Ok(content)
}

/// Check the content hangs together. Runs on the *merged* result, never on
/// one file.
///
/// Fatal, unlike `state::reconcile`: content is authored by hand, so a dangling
/// reference here is a mod bug worth stopping for.
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

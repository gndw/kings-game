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
    pub buildings: IndexMap<String, Building>,
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
            buildings: IndexMap::new(),
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
            self.buildings.insert(building.id.clone(), building);
        }
        for house in file.houses {
            self.houses.insert(house.id.clone(), house);
        }
        for character in file.characters {
            self.characters.insert(character.id.clone(), character);
        }
    }
}

/// The edge of the world, `(x0, y0)` bottom-left to `(x1, y1)` top-right. `world.ron`.
#[derive(Debug, Default, Deserialize)]
pub struct Border {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
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

/// Something built in a holding. Civil buildings earn `gold_profit`; military
/// ones cost `gold_upkeep` and add `levy` troops. A building sets one gold field
/// or the other, never both — the other stays 0.
#[derive(Debug, Deserialize)]
pub struct Building {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub gold_profit: u32,
    #[serde(default)]
    pub gold_upkeep: u32,
    #[serde(default)]
    pub levy: u32,
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

/// `(x_min, x_max, y_min, y_max)` of the map edge, for the canvas bounds.
pub fn bounds(border: &Border) -> (f64, f64, f64, f64) {
    (border.x0, border.x1, border.y0, border.y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bounds() {
        let content = parse(
            r#"(
                // a comment
                border: (x0: -1, y0: 0, x1: 5, y1: 9),
                lands: [
                    (id: "wessex", name: "Wessex", holding: (2, 3), borders: [(1, 2), (3, 4), (1, 2)]),
                    (id: "mercia", name: "Mercia", holding: (2, 4), borders: [(-1, 0), (5, 9)]),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(content.lands.len(), 2);
        assert_eq!(content.lands[0].id, "wessex");
        assert_eq!(
            content.lands[0].borders,
            vec![(1.0, 2.0), (3.0, 4.0), (1.0, 2.0)]
        );
        assert_eq!(bounds(&content.border), (-1.0, 5.0, 0.0, 9.0));
        assert!(parse(r#"(border: (x0: 5, y0: 0, x1: 5, y1: 9), lands: [])"#).is_err());
        assert!(
            parse(r#"(border: (x0: 0, y0: 0, x1: 1, y1: 1), lands: [(id: "l", name: "L", holding: (1, 2), borders: [(1, 2)])])"#)
                .is_err()
        );
        assert!(parse("(border: 3)").is_err());
    }

    #[test]
    fn parses_the_roster() {
        let content = parse(
            r#"(
            border: (x0: 0, y0: 0, x1: 10, y1: 10),
            houses: [(id: "h1", name: "H1")],
            characters: [(id: "c1", name: "C1", house_id: "h1")],
        )"#,
        )
        .unwrap();
        assert_eq!(content.character("c1").unwrap().name, "C1");
        assert_eq!(content.house("h1").unwrap().name, "H1");
        // A house nothing declares is a broken mod, not a repairable save.
        assert!(
            parse(
                r#"(border: (x0: 0, y0: 0, x1: 10, y1: 10),
                   characters: [(id: "c1", name: "C1", house_id: "nowhere")])"#
            )
            .is_err()
        );
    }

}

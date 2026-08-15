//! Everything the mods define plus the starting state: one struct per entity
//! kind, loaded two-phase (definitions merge, then state overlays), then passed
//! whole into `ecs::populate`.

use crate::ecs::{BuildingStatus, CharacterSex, CourtierType};
use crate::resources::border::Border;
use crate::resources::buildings::{BuildingDef, BuildingDefs};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;

/// Merged definitions plus the starting state.
#[derive(Debug)]
pub struct Content {
    pub border: Border,
    /// Calendar (month/year lengths). Mod section + carried resource.
    pub calendar: Calendar,
    /// ID-keyed for O(1) lookup; insertion-ordered for deterministic iteration.
    pub lands: IndexMap<String, Land>,
    /// Read-only building *definition* roster (one entry per kind).
    pub building_defs: BuildingDefs,
    /// Building *instances* — one per built building. State-only.
    pub buildings: IndexMap<String, Building>,
    pub houses: IndexMap<String, House>,
    pub characters: IndexMap<String, Character>,
    /// Family ties between characters: parent↔child and spouse↔spouse.
    /// Definition-only — these don't change in play.
    pub families: IndexMap<String, Family>,
    /// Realms. Wholly state — arrives only via the state overlay.
    pub kingdoms: IndexMap<String, Kingdom>,
    pub courtiers: IndexMap<String, Courtier>,
    /// Definition-only: baked at populate time, never edited.
    pub roads: IndexMap<String, Road>,
}

/// Hand-written because an empty `speeds` list isn't a usable game.
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
            families: IndexMap::new(),
            kingdoms: IndexMap::new(),
            courtiers: IndexMap::new(),
            roads: IndexMap::new(),
        }
    }
}

/// One definition file on disk. Every section is optional. `deny_unknown_fields`
/// so a modder's typo is an error instead of a silently-ignored section.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFile {
    #[serde(default)]
    pub border: Option<Border>,
    #[serde(default)]
    pub calendar: Option<Calendar>,
    #[serde(default)]
    pub lands: Vec<Land>,
    /// The `buildings:` section of a definition file is the catalogue.
    #[serde(default)]
    pub buildings: Vec<BuildingDef>,
    #[serde(default)]
    pub houses: Vec<House>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub families: Vec<Family>,
    #[serde(default)]
    pub roads: Vec<Road>,
}

impl Content {
    /// Fold one definition file in. Same id replaces, new id appends.
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
        for family in file.families {
            self.families.insert(family.id.clone(), family);
        }
        for road in file.roads {
            self.roads.insert(road.id.clone(), road);
        }
    }
}

/// One land's geometry. What stands on a land is no longer a field — each built
/// building is its own entity related to the land via `BuildingOnLand`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Land {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// This land's outline (polyline). Not to be confused with `Content::border`, the world edge.
    #[serde(default)]
    pub borders: Vec<(f64, f64)>,
    /// Seat of power, somewhere inside `borders`.
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

/// A family tie between characters — either a parent/child link or a marriage.
/// One entry per relation, not per household; many entries together describe a
/// family's tree.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub id: String,
    /// Discriminates the entry's shape: `Family` for parent/child links,
    /// `Marriage` for spousal links.
    #[serde(rename = "type")]
    pub family_type: FamilyType,
    // Family-type fields (parent/child link).
    #[serde(default)]
    pub child_character_id: String,
    #[serde(default)]
    pub father_character_id: String,
    #[serde(default)]
    pub mother_character_id: String,
    // Marriage-type fields (spousal link).
    #[serde(default)]
    pub husband_character_id: String,
    #[serde(default)]
    pub wife_character_id: String,
}

/// Discriminator for [`Family`] entries.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum FamilyType {
    Family,
    Marriage,
}

/// One character: definition (name/house) + state (numbers). State fields default
/// to zero on a definition-only entry and are filled in by the state overlay.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub house_id: String,
    /// State: date of birth.
    #[serde(default)]
    pub dob: Date,
    /// State: treasury. Signed, so a script may spend past zero.
    #[serde(default)]
    pub gold: i64,
    /// State: troops currently raised.
    #[serde(default)]
    pub levy: u64,
    /// State: gold per month — profit less upkeep. Signed. Recomputed on load.
    #[serde(default)]
    pub gold_yield: i64,
    /// State: whether the character is still alive. Defaults to `true`; flips to `false` on death.
    #[serde(default = "default_alive")]
    pub is_alive: bool,
    /// State: date of death — `None` while alive, set once the character dies.
    #[serde(default)]
    pub death_date: Option<Date>,
    /// Definition: `"m"` / `"f"`. State files omit it (never changes in play).
    #[serde(default)]
    pub sex: CharacterSex,
}

/// `bool::default()` is `false`; alive is the natural starting state.
fn default_alive() -> bool {
    true
}

/// One built building instance: which def it instantiates, which land it stands on, its status.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    pub id: String,
    /// Key into `Content::building_defs`.
    pub def_id: String,
    pub land_id: String,
    #[serde(default)]
    pub status: BuildingStatus,
}

/// A realm: a ruler and the single land it holds. Wholly state.
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

/// A connection between two lands: polyline + two land ids + marching duration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Road {
    pub id: String,
    #[serde(default)]
    pub points: Vec<(f64, f64)>,
    /// Exactly two land ids — the two lands the road joins.
    #[serde(default)]
    pub between_land_ids: Vec<String>,
    /// Days an army spends marching this road. Authored, not derived from `points`.
    #[serde(default)]
    pub distance_days: u32,
}

impl Content {
    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.get(id)
    }
}

/// One definition file. No cross-reference checking — that waits for `validate`.
pub fn parse_file(text: &str) -> Result<ContentFile> {
    let opts =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    Ok(opts.from_str(text)?)
}

/// Check the merged content hangs together. Fatal — content is hand-authored.
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
    for (_, f) in &content.families {
        let check_char = |label: &str, id: &str| -> Result<()> {
            if !content.characters.contains_key(id) {
                bail!("family `{}` {} references unknown character `{id}`", f.id, label);
            }
            Ok(())
        };
        match f.family_type {
            FamilyType::Family => {
                check_char("child", &f.child_character_id)?;
                check_char("father", &f.father_character_id)?;
                check_char("mother", &f.mother_character_id)?;
            }
            FamilyType::Marriage => {
                check_char("husband", &f.husband_character_id)?;
                check_char("wife", &f.wife_character_id)?;
            }
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
        // A free road would let an army teleport (begin and arrive the same day).
        if r.distance_days == 0 {
            bail!("road `{}` needs a `distance_days` of at least 1", r.id);
        }
    }
    Ok(())
}

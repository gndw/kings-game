//! Everything the mods define plus the starting state: one struct per entity
//! kind, loaded two-phase (definitions merge, then state overlays), then passed
//! whole into `ecs::populate`.
//!
//! Events live outside this struct — they're Rhai scripts loaded by `mods::load`
//! (third pass) and compiled into `ScriptedEvent`s on the `EventScripts` resource.
//! See `docs/architecture.md` and `src/scripted_event.rs`.

use crate::ecs::{BuildingStatus, CharacterGender, CourtierType, MemoryKind};
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
    /// One entry per memory entity. Lives in state (initial historical
    /// memories arrive via the state overlay); runtime-created memories
    /// spawned by commands land here too.
    pub memories: IndexMap<String, Memory>,
    /// Definition-only: baked at populate time, never edited.
    pub roads: IndexMap<String, Road>,
    /// State-only: persisted next-due date for the event popup. The trigger
    /// tick reads `Content::event_deck::next_due_date` at startup; the resolver
    /// rewrites it after each event resolves or the player forfeits.
    pub event_deck: EventDeckState,
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
            memories: IndexMap::new(),
            roads: IndexMap::new(),
            event_deck: EventDeckState::default(),
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
    #[serde(default)]
    pub memories: Vec<Memory>,
    /// State-shaped (the date a trigger should fire next); only meant to be
    /// shipped in a state file. Definition files typically leave it absent.
    #[serde(default)]
    pub event_deck: Option<EventDeckState>,
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
        for memory in file.memories {
            self.memories.insert(memory.id.clone(), memory);
        }
        if let Some(event_deck) = file.event_deck {
            self.event_deck = event_deck;
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

/// One character: definition (name/house/skills) + state (numbers). State
/// fields default to zero on a definition-only entry and are filled in by the
/// state overlay.
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
    /// State: when this character is next due for a death roll. Older chars get
    /// shorter horizons (see [`random_horizon_days`]).
    #[serde(default)]
    pub next_death_event_date: Date,
    /// Definition: `"m"` / `"f"`. State files omit it (never changes in play).
    #[serde(default)]
    pub gender: CharacterGender,
    /// Authored baseline + state overlay: the character's six abilities.
    /// Skill values are clamped into `0..=20` at populate time, so a state
    /// save can drift a stat without breaking the definition.
    #[serde(default)]
    pub skills: Skills,
}

/// A character's six abilities. Authored as definitions so a mod can rebalance
/// a whole house by editing one file; the state overlay may replace any of
/// them so a "Wounded" / "Well-taught" trait can shift current values without
/// breaking the definition.
///
/// The range is 0..=20 across all six. Definition-time values outside the
/// range fail `validate`; state-overlay values are clamped silently in
/// `populate`.
#[derive(Debug, Default, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct Skills {
    /// Field command, battle tactics, siegecraft. Folded with logistics:
    /// affects monthly levy replenishment, march distance, and army combat.
    #[serde(default)]
    pub martial: i32,
    /// Personal combat, duels, ambushes, surviving assassination. Drives
    /// the monthly personal safety check.
    #[serde(default)]
    pub prowess: i32,
    /// Tax efficiency plus trade leverage. Monthly gold yield multiplier.
    #[serde(default)]
    pub treasury: i32,
    /// Internal judgment plus external accord. Monthly vassal and foreign
    /// opinion drift.
    #[serde(default)]
    pub prudence: i32,
    /// Plots, detection, secrets. Monthly plot-detection threshold and
    /// rumor spread.
    #[serde(default)]
    pub intrigue: i32,
    /// Piety plus theological literacy. Church favor and legitimacy drift,
    /// event-tier unlocks.
    #[serde(default)]
    pub faith: i32,
}

impl Skills {
    /// The minimum / maximum a single skill can hold. Values are clamped into
    /// the half-open range `[0, 20]` at populate time.
    pub const MIN: i32 = 0;
    pub const MAX: i32 = 20;

    /// True if every skill lies in `MIN..=MAX`. `validate` calls this on
    /// each character's authored skill block.
    pub fn in_range(&self) -> bool {
        self.martial >= Self::MIN
            && self.martial <= Self::MAX
            && self.prowess >= Self::MIN
            && self.prowess <= Self::MAX
            && self.treasury >= Self::MIN
            && self.treasury <= Self::MAX
            && self.prudence >= Self::MIN
            && self.prudence <= Self::MAX
            && self.intrigue >= Self::MIN
            && self.intrigue <= Self::MAX
            && self.faith >= Self::MIN
            && self.faith <= Self::MAX
    }

    /// Clamp every skill into `MIN..=MAX`, returning a new value.
    pub fn clamped(&self) -> Self {
        let c = |v: i32| v.clamp(Self::MIN, Self::MAX);
        Self {
            martial: c(self.martial),
            prowess: c(self.prowess),
            treasury: c(self.treasury),
            prudence: c(self.prudence),
            intrigue: c(self.intrigue),
            faith: c(self.faith),
        }
    }
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
    /// Display name. Defaults to `"Kingdom of <land name>"` at populate
    /// time when the state file leaves it blank — see `ecs::populate`.
    #[serde(default)]
    pub name: String,
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

/// A character carries memories about other characters' deeds. Each memory
/// entity is a per-recipient record: who gave what, when, until when. The
/// opinion helper reads active memories to add their contribution to the
/// score one character holds toward another.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub id: String,
    /// The character who OWNS this memory (the recipient of the deed).
    pub character_id: String,
    /// The character the memory is ABOUT (the actor whose deed is remembered).
    pub toward_character_id: String,
    #[serde(default)]
    pub created_date: Date,
    #[serde(default)]
    pub until_date: Date,
    pub kind: MemoryKind,
}

/// State for the event popup trigger. Persisted by `*.state.ron` so a save
/// reload (or a mod that wants a different first-event date) can rewrite the
/// `next_due_date` to a specific game day. The runtime `EventDeck` resource
/// in `game::presenting_event` reads this through `main.rs` at startup; the
/// resolver rewrites `Content::event_deck::next_due_date` is not currently
/// wired (saves aren't yet a thing), so today this is one-shot at load.
#[derive(Debug, Default, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct EventDeckState {
    /// Day the trigger tick should next consider firing. State files must
    /// provide this date.
    #[serde(default)]
    pub next_due_date: Date,
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
        if !c.skills.in_range() {
            bail!(
                "character `{}` has out-of-range skills (must be 0..=20): {:?}",
                c.id,
                c.skills
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
    for (_, m) in &content.memories {
        if !content.characters.contains_key(&m.character_id) {
            bail!(
                "memory `{}` references unknown character_id `{}`",
                m.id,
                m.character_id
            );
        }
        if !content.characters.contains_key(&m.toward_character_id) {
            bail!(
                "memory `{}` references unknown toward_character_id `{}`",
                m.id,
                m.toward_character_id
            );
        }
    }
    Ok(())
}

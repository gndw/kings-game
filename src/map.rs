//! Map geometry loaded from RON data files at startup so it can be modded
//! without a rebuild (see `mods/base/`). `crate::mods` does the loading and
//! merging; the camera and drawing live in `crate::ui::map`.

use crate::date::Calendar;
use anyhow::{Result, bail};
use rand::seq::IndexedRandom;
use serde::Deserialize;

/// The whole map after every mod file has been merged in: a rectangular
/// `border` — the edge of the world — and the `lands` inside it.
// Default so tests that only care about the clock can build a Ctx with an empty
// map, and so merging can start from nothing.
#[derive(Debug)]
pub struct Map {
    pub border: Border,
    /// How long a month and a year are. Not geometry, but it arrives the same
    /// way every other mod section does.
    pub calendar: Calendar,
    /// The simulated-days-per-real-second settings `+` and `-` step through,
    /// slowest first. The game starts on the first one.
    pub speeds: Vec<u32>,
    pub lands: Vec<Land>,
    /// Buildings a holding can raise.
    pub buildings: Vec<Building>,
    pub houses: Vec<House>,
    pub characters: Vec<Character>,
    pub kingdoms: Vec<Kingdom>,
}

/// Hand-written rather than derived because an empty `speeds` list is not a
/// usable game — a derived `Default` would hand out one silently.
impl Default for Map {
    fn default() -> Self {
        Map {
            border: Border::default(),
            calendar: Calendar::default(),
            speeds: vec![8, 16, 32, 64],
            lands: Vec::new(),
            buildings: Vec::new(),
            houses: Vec::new(),
            characters: Vec::new(),
            kingdoms: Vec::new(),
        }
    }
}

/// One data file on disk. Every section is optional, so a mod ships only what
/// it changes — and the base game can split itself across `lands.ron`,
/// `buildings.ron` and friends without the loader knowing the difference.
///
/// `deny_unknown_fields` so a modder's typo is an error instead of a section
/// that silently does nothing.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapFile {
    #[serde(default)]
    pub border: Option<Border>,
    #[serde(default)]
    pub calendar: Option<Calendar>,
    /// Replaced wholesale, not merged — a speed has no id to match on.
    #[serde(default)]
    pub speeds: Option<Vec<u32>>,
    #[serde(default)]
    pub lands: Vec<Land>,
    #[serde(default)]
    pub buildings: Vec<Building>,
    #[serde(default)]
    pub houses: Vec<House>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub kingdoms: Vec<Kingdom>,
}

/// ponytail: linear scan per entry, so merging is O(n²) in list length. With a
/// few hundred lands that's nothing; index by id if a mod set ever gets big.
fn merge_by_id<T>(dst: &mut Vec<T>, src: Vec<T>, id: impl for<'a> Fn(&'a T) -> &'a str) {
    for item in src {
        match dst.iter().position(|d| id(d) == id(&item)) {
            Some(i) => dst[i] = item,
            None => dst.push(item),
        }
    }
}

impl Map {
    /// Fold one file in. An entry whose `id` already exists replaces the
    /// earlier one, anything else appends — that is the whole override rule.
    pub fn merge(&mut self, file: MapFile) {
        if let Some(border) = file.border {
            self.border = border;
        }
        if let Some(calendar) = file.calendar {
            self.calendar = calendar;
        }
        if let Some(speeds) = file.speeds {
            self.speeds = speeds;
        }
        merge_by_id(&mut self.lands, file.lands, |s| &s.id);
        merge_by_id(&mut self.buildings, file.buildings, |b| &b.id);
        merge_by_id(&mut self.houses, file.houses, |h| &h.id);
        merge_by_id(&mut self.characters, file.characters, |c| &c.id);
        merge_by_id(&mut self.kingdoms, file.kingdoms, |k| &k.id);
    }

    /// Get random land id, or None if there are no lands.
    pub fn random_land_id(&self) -> Option<String> {
        self.lands.choose(&mut rand::rng()).map(|s| s.id.clone())
    }

    /// The land to move the selection to when stepping from `from` along `dir`
    /// (a unit-ish direction). Picks the nearest holding that lies in that
    /// direction, penalising sideways offset so "up" prefers straight up.
    ///
    /// ponytail: distance heuristic over holdings, no adjacency graph. Add real
    /// borders-touch adjacency in lands.ron if the picks feel wrong on odd shapes.
    pub fn step(&self, from: &str, dir: (f64, f64)) -> Option<String> {
        let origin = self.lands.iter().find(|s| s.id == from)?.holding;
        self.lands
            .iter()
            .filter(|s| s.id != from)
            .filter_map(|s| {
                let (dx, dy) = (s.holding.0 - origin.0, s.holding.1 - origin.1);
                let along = dx * dir.0 + dy * dir.1;
                // Perpendicular component: how far off-axis the candidate sits.
                let perp = (dx * dir.1 - dy * dir.0).abs();
                (along > perp).then(|| (along + perp * 2.0, s.id.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }
}

/// Map edge, `(x0, y0)` bottom-left to `(x1, y1)` top-right. `world.ron`.
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
    /// confused with `Map::border`, the edge of the world.
    pub borders: Vec<(f64, f64)>,
    /// Seat of power, somewhere inside `borders`. Drawn as a circle.
    pub holding: (f64, f64),
    /// Ids into `Map::buildings` — what already stands in this land.
    #[serde(default)]
    pub building_ids: Vec<String>,
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

#[derive(Debug, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub house_id: String,
    pub age: u32,
    /// Treasury. Signed, so a script may spend past zero. Starts at whatever
    /// the data says, then the sim owns it.
    #[serde(default)]
    pub gold: i64,
    /// Troops currently raised. Only a character who leads a kingdom has
    /// holdings to raise them from.
    #[serde(default)]
    pub levy: u64,
}

/// What a realm's holdings yield — troops raised, coin earned, coin owed. The
/// raw sums; see [`Map::kingdom_yield`]. All zeroes for a character who leads
/// no kingdom, which is what keeps gold and levy to rulers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Yield {
    pub levy: u64,
    pub gold_profit: u64,
    pub gold_upkeep: u64,
}

/// A realm: a ruler, a capital, and the lands it holds.
#[derive(Debug, Deserialize)]
pub struct Kingdom {
    pub id: String,
    pub leader_character_id: String,
    pub seat_land_id: String,
    pub land_ids: Vec<String>,
}

impl Map {
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

    pub fn building(&self, id: &str) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }

    /// What everything a kingdom holds adds up to. The raw sums only — whether
    /// income is profit or profit-minus-upkeep is a mod script's call, not
    /// this function's.
    pub fn kingdom_yield(&self, kingdom: &Kingdom) -> Yield {
        let mut total = Yield::default();
        for land in self
            .lands
            .iter()
            .filter(|l| kingdom.land_ids.contains(&l.id))
        {
            for b in land.building_ids.iter().filter_map(|id| self.building(id)) {
                total.levy += u64::from(b.levy);
                total.gold_profit += u64::from(b.gold_profit);
                total.gold_upkeep += u64::from(b.gold_upkeep);
            }
        }
        total
    }

    pub fn character(&self, id: &str) -> Option<&Character> {
        self.characters.iter().find(|c| c.id == id)
    }

    /// For the sim to write a character's gold and levy back.
    pub fn character_mut(&mut self, id: &str) -> Option<&mut Character> {
        self.characters.iter_mut().find(|c| c.id == id)
    }

    pub fn house(&self, id: &str) -> Option<&House> {
        self.houses.iter().find(|h| h.id == id)
    }
}

/// One data file. No cross-reference checking — a mod may legitimately point at
/// a building some other mod declares, so that waits for [`validate`].
pub fn parse_file(text: &str) -> Result<MapFile> {
    // IMPLICIT_SOME so an optional section is written `border: (...)` rather
    // than `border: Some((...))` — modders shouldn't have to know which
    // sections happen to be `Option` on the Rust side.
    let opts =
        ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    Ok(opts.from_str(text)?)
}

/// A whole map from a single file, for tests and one-file mods. The game merges
/// many instead, via `crate::mods::load`.
pub fn parse(text: &str) -> Result<Map> {
    let mut map = Map::default();
    map.merge(parse_file(text)?);
    validate(&map)?;
    Ok(map)
}

/// Check the map hangs together. Runs on the *merged* map, never on one file.
pub fn validate(map: &Map) -> Result<()> {
    let b = &map.border;
    if b.x1 <= b.x0 || b.y1 <= b.y0 {
        bail!("map border must have x1 > x0 and y1 > y0");
    }
    map.calendar.validate()?;
    match map.speeds.as_slice() {
        [] => bail!("speeds needs at least one entry"),
        s if s.contains(&0) => bail!("a speed of 0 days/second would stop the clock"),
        _ => {}
    }
    for s in &map.lands {
        if s.borders.len() < 2 {
            bail!("land `{}` needs at least 2 border points", s.id);
        }
        for b in &s.building_ids {
            if !map.buildings.iter().any(|d| &d.id == b) {
                bail!("land `{}` references unknown building `{}`", s.id, b);
            }
        }
    }
    for c in &map.characters {
        if !map.houses.iter().any(|h| h.id == c.house_id) {
            bail!(
                "character `{}` references unknown house `{}`",
                c.id,
                c.house_id
            );
        }
    }
    for k in &map.kingdoms {
        if map.character(&k.leader_character_id).is_none() {
            bail!(
                "kingdom `{}` references unknown character `{}`",
                k.id,
                k.leader_character_id
            );
        }
        for l in &k.land_ids {
            if !map.lands.iter().any(|s| &s.id == l) {
                bail!("kingdom `{}` references unknown land `{}`", k.id, l);
            }
        }
        if !k.land_ids.contains(&k.seat_land_id) {
            bail!(
                "kingdom `{}` seat `{}` is not among its lands",
                k.id,
                k.seat_land_id
            );
        }
    }
    Ok(())
}

/// `(x_min, x_max, y_min, y_max)` of the map edge, for the canvas bounds.
pub fn bounds(map: &Map) -> (f64, f64, f64, f64) {
    let b = &map.border;
    (b.x0, b.x1, b.y0, b.y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bounds() {
        let map = parse(
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
        assert_eq!(map.lands.len(), 2);
        assert_eq!(map.lands[0].id, "wessex");
        assert_eq!(
            map.lands[0].borders,
            vec![(1.0, 2.0), (3.0, 4.0), (1.0, 2.0)]
        );
        assert_eq!(bounds(&map), (-1.0, 5.0, 0.0, 9.0));
        assert!(["wessex", "mercia"].contains(&map.random_land_id().unwrap().as_str()));
        assert!(parse(r#"(border: (x0: 5, y0: 0, x1: 5, y1: 9), lands: [])"#).is_err());
        assert!(
            parse(r#"(border: (x0: 0, y0: 0, x1: 1, y1: 1), lands: [(id: "l", name: "L", holding: (1, 2), borders: [(1, 2)])])"#)
                .is_err()
        );
        assert!(parse("(border: 3)").is_err());
    }

    #[test]
    fn parses_kingdoms() {
        let text = |seat: &str| {
            format!(
                r#"(
                border: (x0: 0, y0: 0, x1: 10, y1: 10),
                lands: [(id: "l1", name: "L1", holding: (1, 1), borders: [(1, 1), (2, 2)])],
                houses: [(id: "h1", name: "H1")],
                characters: [(id: "c1", name: "C1", house_id: "h1", age: 40)],
                kingdoms: [(id: "k1", leader_character_id: "c1", seat_land_id: "{seat}", land_ids: ["l1"])],
            )"#
            )
        };
        let map = parse(&text("l1")).unwrap();
        assert_eq!(map.kingdom_of("l1").unwrap().id, "k1");
        assert!(map.kingdom_of("nowhere").is_none());
        assert_eq!(map.kingdom_led_by("c1").unwrap().seat_land_id, "l1");
        assert!(map.kingdom_led_by("nobody").is_none());
        assert_eq!(map.character("c1").unwrap().age, 40);
        assert_eq!(map.house("h1").unwrap().name, "H1");
        // a seat outside the kingdom's own lands is a broken map
        assert!(parse(&text("l2")).is_err());
    }

    #[test]
    fn steps_between_lands() {
        let map = parse(
            r#"(
                border: (x0: 0, y0: 0, x1: 10, y1: 10),
                lands: [
                    (id: "mid", name: "mid", holding: (5, 5), borders: [(5, 5), (5, 5)]),
                    (id: "east", name: "east", holding: (8, 5), borders: [(8, 5), (8, 5)]),
                    (id: "far_east", name: "far_east", holding: (9, 5), borders: [(9, 5), (9, 5)]),
                    (id: "north", name: "north", holding: (5, 9), borders: [(5, 9), (5, 9)]),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(map.step("mid", (1.0, 0.0)).as_deref(), Some("east"));
        assert_eq!(map.step("east", (1.0, 0.0)).as_deref(), Some("far_east"));
        assert_eq!(map.step("mid", (0.0, 1.0)).as_deref(), Some("north"));
        assert_eq!(map.step("north", (0.0, 1.0)), None);
        // Nothing west of mid, and an unknown land can't step.
        assert_eq!(map.step("mid", (-1.0, 0.0)), None);
        assert_eq!(map.step("nowhere", (1.0, 0.0)), None);
    }
}

//! Mods. Each folder under the mods directory contributes data files; folders
//! load in sorted name order and later ones win.
//!
//! Every `*.ron` in a folder is a [`ContentFile`], whatever it's called — the
//! loader doesn't know that `buildings.ron` holds buildings. The one exception
//! is `*.state.ron`, which is a [`State`]: the mutable half of the world, and
//! the shape a save file will have. So a mod splits itself across files however
//! reads best and the loader neither knows nor cares.
//!
//! Modding scripting (rhai hooks) has been pulled out and will be rebuilt after
//! the ECS refactoring; `*.rhai` files are ignored for now.

#[cfg(test)]
mod testkit;

use crate::content::{self, Content};
use crate::state::{self, State};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Mods {
    pub content: Content,
    pub state: State,
}

/// Load every mod in `dir`. Bad *data* is fatal — there's no sensible game
/// without content. Bad *state* is repaired rather than refused; see
/// [`state::reconcile`].
pub fn load(dir: &Path) -> Result<Mods> {
    let mut content = Content::default();
    let mut state = State::default();

    for folder in sorted_entries(dir)? {
        if !folder.is_dir() {
            continue;
        }
        for file in sorted_entries(&folder)? {
            let Some(ext) = file.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let read = || {
                std::fs::read_to_string(&file)
                    .with_context(|| format!("reading {}", file.display()))
            };
            let is_state = file
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".state.ron"));
            match ext {
                "ron" if is_state => {
                    let parsed = state::parse_file(&read()?)
                        .with_context(|| format!("parsing {}", file.display()))?;
                    state.merge(parsed);
                }
                "ron" => {
                    let text = read()?;
                    let parsed = content::parse_file(&text)
                        .with_context(|| format!("parsing {}", file.display()))?;
                    content.merge(parsed);
                }
                _ => {}
            }
        }
    }

    content::validate(&content).context("the merged mod data is inconsistent")?;
    // State goes last: it can only be lined up once every mod has had its say
    // about what exists. Repairs are chronicled rather than fatal — see
    // `state::reconcile`.
    for note in state::reconcile(&content, &mut state) {
        eprintln!("state: {note}");
    }
    Ok(Mods { content, state })
}

/// Directory contents, sorted by path. Sorted because load order decides who
/// overrides whom *and* how many RNG draws happen — both have to be
/// reproducible or saves and replays drift.
fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn later_mods_override_by_id_and_append() {
        let dir = mods_dir(
            "override",
            &[
                ("base/world.ron", WORLD),
                ("base/lands.ron", LAND_1),
                (
                    "zz-bigger/lands.ron",
                    r#"(lands: [
                        (id: "land-1", name: "renamed", holding: (3, 3), borders: [(3, 3), (4, 4)]),
                        (id: "land-2", name: "added", holding: (5, 5), borders: [(5, 5), (6, 6)]),
                    ])"#,
                ),
            ],
        );
        let map = load(&dir).unwrap().content;
        assert_eq!(
            map.lands.len(),
            2,
            "land-1 replaced in place, land-2 appended"
        );
        assert_eq!(map.lands[0].id, "land-1");
        assert_eq!(map.lands[0].name, "renamed");
        assert_eq!(map.lands[1].name, "added");
        // The override kept its position, so ordering is stable across mods.
        assert_eq!(map.border.x1, 10.0);
    }

    #[test]
    fn splitting_a_mod_across_files_changes_nothing() {
        let split = load(&mods_dir(
            "split",
            &[
                ("base/a-world.ron", WORLD),
                ("base/b-buildings.ron", MILL),
                ("base/c-lands.ron", LAND_1),
                ("base/d-start.state.ron", LAND_1_MILL),
            ],
        ))
        .unwrap();
        let whole = load(&mods_dir(
            "whole",
            &[
                (
                    "base/all.ron",
                    r#"(border: (x0: 0, y0: 0, x1: 10, y1: 10),
                    buildings: [(id: "b-mill", name: "mill", gold_profit: 6)],
                    lands: [(id: "land-1", name: "first", holding: (1, 1),
                             borders: [(1, 1), (2, 2)])])"#,
                ),
                ("base/all.state.ron", LAND_1_MILL),
            ],
        ))
        .unwrap();
        assert_eq!(
            format!("{:?}", split.content),
            format!("{:?}", whole.content)
        );
        assert_eq!(format!("{:?}", split.state), format!("{:?}", whole.state));
    }

    #[test]
    fn a_mod_can_replace_the_calendar() {
        // No calendar.ron anywhere, so the default 30/12 stands.
        let plain = load(&mods_dir("cal-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .content;
        assert_eq!(plain.calendar.days_per_year(), 360);

        // A mod that ships nothing but a calendar still overrides it.
        let dir = mods_dir(
            "cal-short",
            &[
                ("base/world.ron", WORLD),
                (
                    "z-short-year/calendar.ron",
                    "(calendar: (days_per_month: 10, months_per_year: 5))",
                ),
            ],
        );
        let map = load(&dir).unwrap();
        assert_eq!(map.content.calendar.days_per_year(), 50);

        // ...and the sim actually runs on it.
        let calendar = map.content.calendar;
        let mut date = crate::resources::date::Date::START;
        for _ in 0..50 {
            crate::updates::tick::advance(&mut date, &calendar);
        }
        assert_eq!(date.year, 1067);
        assert_eq!((date.month, date.day), (1, 1));

        // A calendar that would never roll over is refused at load.
        let broken = mods_dir(
            "cal-zero",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/calendar.ron",
                    "(calendar: (days_per_month: 0, months_per_year: 12))",
                ),
            ],
        );
        assert!(load(&broken).is_err());
    }

    #[test]
    fn a_mod_can_replace_the_speed_list() {
        let plain = load(&mods_dir("spd-default", &[("base/world.ron", WORLD)]))
            .unwrap()
            .content;
        assert_eq!(plain.speeds, vec![8, 16, 32, 64]);

        // Replaced wholesale, not appended to.
        let map = load(&mods_dir(
            "spd-slow",
            &[
                ("base/world.ron", WORLD),
                ("z-slow/calendar.ron", "(speeds: [1, 2])"),
            ],
        ))
        .unwrap()
        .content;
        assert_eq!(map.speeds, vec![1, 2]);

        // An empty list leaves nothing to run at, and a zero would stop the
        // clock dead — both are refused at load rather than at the keyboard.
        for bad in ["(speeds: [])", "(speeds: [8, 0])"] {
            let dir = mods_dir("spd-bad", &[("base/world.ron", WORLD), ("base/s.ron", bad)]);
            assert!(load(&dir).is_err(), "{bad} must not be accepted");
        }
    }

    #[test]
    fn a_mod_may_reference_another_mods_building() {
        // The state puts `b-mill` in land-1, and only the *second* folder
        // declares that building. Works because state is lined up after every
        // mod has merged, not while they load.
        let mods = load(&mods_dir(
            "cross",
            &[
                ("a-lands/world.ron", WORLD),
                ("a-lands/lands.ron", LAND_1),
                ("a-lands/start.state.ron", LAND_1_MILL),
                ("b-buildings/buildings.ron", MILL),
            ],
        ))
        .unwrap();
        assert_eq!(mods.state.buildings_in("land-1"), ["b-mill"]);

        // A building nothing ever declares is dropped and chronicled, not
        // fatal: the same state may load again once that mod is back.
        let orphan = load(&mods_dir(
            "orphan",
            &[
                ("base/world.ron", WORLD),
                ("base/lands.ron", LAND_1),
                ("base/start.state.ron", LAND_1_MILL),
            ],
        ))
        .unwrap();
        assert!(orphan.state.buildings_in("land-1").is_empty());
    }

    /// The split's promise, end to end: a save that predates a mod's new land
    /// and character still loads, and the new content starts where it says.
    #[test]
    fn state_from_before_the_content_still_loads() {
        let mods = load(&mods_dir(
            "old-save",
            &[
                ("base/world.ron", WORLD),
                ("base/buildings.ron", MILL),
                ("base/lands.ron", LAND_1),
                ("base/houses.ron", r#"(houses: [(id: "h1", name: "H1")])"#),
                (
                    "base/characters.ron",
                    r#"(characters: [(id: "c1", name: "C1", house_id: "h1")])"#,
                ),
                // The "save": written when only land-1 existed.
                ("base/a-save.state.ron", LAND_1_MILL),
                // Content added since, with its own starting state.
                (
                    "z-expansion/lands.ron",
                    r#"(lands: [(id: "land-2", name: "new", holding: (3, 3), borders: [(3, 3), (4, 4)])])"#,
                ),
                (
                    "z-expansion/characters.ron",
                    r#"(characters: [(id: "c2", name: "C2", house_id: "h1")])"#,
                ),
                (
                    "z-expansion/start.state.ron",
                    r#"(lands: [(id: "land-2", building_ids: ["b-mill"])],
                        characters: [(id: "c2", age: 20, gold: 5)])"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(mods.state.buildings_in("land-1"), ["b-mill"], "the save");
        assert_eq!(
            mods.state.buildings_in("land-2"),
            ["b-mill"],
            "the new land"
        );
        assert_eq!(mods.state.character("c2").unwrap().gold, 5);
        assert_eq!(
            mods.state.character("c1").unwrap().gold,
            0,
            "a character the save never mentioned defaults"
        );
    }

    #[test]
    fn unknown_sections_are_rejected() {
        let dir = mods_dir(
            "typo",
            &[("base/world.ron", WORLD), ("base/x.ron", "(buildingz: [])")],
        );
        assert!(
            load(&dir).is_err(),
            "a typo'd section must not pass silently"
        );
    }
}

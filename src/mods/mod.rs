//! Mods. Each folder under the mods directory contributes data files; folders
//! load in sorted name order and later ones win.
//!
//! Three passes per folder:
//!
//! 1. **Definitions** — every `*.ron` that isn't `*.state.ron` merges into
//!    `Content`.
//! 2. **State** — every `*.state.ron` overlays the mutable half of the same
//!    structs field by field.
//! 3. **Event scripts** — every `event-<id>.rhai` compiles into a
//!    `ScriptedEvent`; last-loaded-wins on the id (so a mod can replace a
//!    base event by shipping one with the same filename).
//!
//! Modding scripting was pulled out and rebuilt — `event-<id>.rhai` files
//! are the wedge, with `scripted_event::ScriptedEvent` as the contract.

use crate::content::{self, Content};
use crate::resources::event_scripts::EventScripts;
use crate::script_ctx;
use crate::scripted_event::ScriptedEvent;
use crate::state;
use anyhow::{Context, Result};
use rhai::Engine;
use std::path::{Path, PathBuf};

pub struct Mods {
    pub content: Content,
    pub event_scripts: EventScripts,
}

/// Load every mod in `dir`. Bad *data* is fatal — there's no sensible game
/// without content. Bad *state* is repaired rather than refused; see
/// [`state::reconcile`]. Bad *scripts* are reported and skipped (so a typo
/// in one event doesn't lock out the rest).
pub fn load(dir: &Path) -> Result<Mods> {
    let mut content = Content::default();
    let mut state_files: Vec<PathBuf> = Vec::new();
    let mut script_files: Vec<PathBuf> = Vec::new();

    // Pass 1+2: collect RON paths; pass 3: collect script paths. Walk once.
    for folder in sorted_entries(dir)? {
        if !folder.is_dir() {
            continue;
        }
        for file in sorted_entries(&folder)? {
            let Some(ext) = file.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            match ext {
                "ron" => {
                    let name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if name.ends_with(".state.ron") {
                        state_files.push(file);
                    } else {
                        let text = read_to_string(&file)?;
                        let parsed = content::parse_file(&text)
                            .with_context(|| format!("parsing {}", file.display()))?;
                        content.merge(parsed);
                    }
                }
                "rhai" => script_files.push(file),
                _ => {}
            }
        }
    }
    content::validate(&content).context("the merged mod data is inconsistent")?;

    // Pass 2: state overlays onto the now-complete content. Repairs are
    // chronicled rather than fatal — see `state::reconcile`.
    for file in state_files {
        let text = read_to_string(&file)?;
        let parsed = state::parse_file(&text)
            .with_context(|| format!("parsing {}", file.display()))?;
        content.merge_state(parsed);
    }
    for note in state::reconcile(&mut content) {
        eprintln!("state: {note}");
    }

    // Pass 3: event scripts. Build the engine once, register the API, then
    // compile each file. Last-loaded-wins on the id so a mod can replace a
    // base event by shipping the same filename.
    let mut engine = Engine::new();
    script_ctx::register_api(&mut engine);
    let mut events: Vec<ScriptedEvent> = Vec::new();
    for file in script_files {
        match ScriptedEvent::load(&engine, &file) {
            Ok(ev) => {
                if let Some(slot) = events.iter_mut().find(|e| e.id == ev.id) {
                    *slot = ev;
                } else {
                    events.push(ev);
                }
            }
            Err(e) => {
                eprintln!("event script: {e:?}");
            }
        }
    }

    Ok(Mods {
        content,
        event_scripts: EventScripts { engine, events },
    })
}

fn read_to_string(file: &Path) -> Result<String> {
    std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))
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

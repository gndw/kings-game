//! Mods. Each folder under the mods directory contributes data files; folders
//! load in sorted name order and later ones win.
//!
//! Every `*.ron` in a folder is a [`ContentFile`], whatever it's called — the
//! loader doesn't know that `buildings.ron` holds buildings. The one exception
//! is `*.state.ron`, which is a [`StateFile`]: the mutable half of the world,
//! overlaid onto the content structs, and the shape a save file will have. So a
//! mod splits itself across files however reads best and the loader neither
//! knows nor cares.
//!
//! Loading is two-pass per the content/state contract: all definition files
//! merge first, then the state files overlay onto the same structs — so state
//! can only ever fill entries the definitions established.
//!
//! Modding scripting (rhai hooks) has been pulled out and will be rebuilt after
//! the ECS refactoring; `*.rhai` files are ignored for now.

use crate::content::{self, Content};
use crate::state;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Mods {
    pub content: Content,
}

/// Load every mod in `dir`. Bad *data* is fatal — there's no sensible game
/// without content. Bad *state* is repaired rather than refused; see
/// [`state::reconcile`].
pub fn load(dir: &Path) -> Result<Mods> {
    let mut content = Content::default();
    let mut state_files: Vec<PathBuf> = Vec::new();

    // Pass 1: definitions. State files are just collected.
    for folder in sorted_entries(dir)? {
        if !folder.is_dir() {
            continue;
        }
        for file in sorted_entries(&folder)? {
            let Some(ext) = file.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if ext != "ron" {
                continue;
            }
            let is_state = file
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".state.ron"));
            if is_state {
                state_files.push(file);
            } else {
                let text = read_to_string(&file)?;
                let parsed = content::parse_file(&text)
                    .with_context(|| format!("parsing {}", file.display()))?;
                content.merge(parsed);
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
    Ok(Mods { content })
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

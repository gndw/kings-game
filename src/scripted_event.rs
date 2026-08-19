//! One Rhai-scripted event. Loaded from `event-<id>.rhai` files by
//! `mods::load`; the runtime calls the exported functions at each step.
//!
//! Required top-level functions in every event script:
//! - `title() -> String`
//! - `narration() -> String`
//! - `weight() -> i64`
//! - `choices() -> Array` of `#{ text: String, chronicle: String }` records
//!
//! Optional (default if absent):
//! - `can_trigger(world) -> bool` — eligibility gate; default `true`
//! - `characters(world) -> Array` — the characters this event is about, in
//!   order; the script is responsible for any RNG pick (via
//!   `world.ctx.rng`). Default empty.
//! - `effect(world) -> ()` — mechanical effect; default no-op
//! - `decline() -> String` — per-event chronicle line; default generic
//!
//! The id is the filename (sans `event-` prefix and `.rhai`). `events` are
//! merged across mod folders with last-loaded-wins on the id.

use anyhow::{Context, Result, bail};
use rhai::{AST, Engine, Scope};
use std::path::Path;
use std::sync::Arc;

/// A choice row returned by the script's `choices()` function. `chronicle`
/// is `None` for choices whose mechanical effect will write its own line
/// (e.g. a gold transfer triggers `on_gold_gifted`).
#[derive(Debug, Clone)]
pub struct ChoiceRow {
    pub text: String,
    pub chronicle: Option<String>,
}

/// One event as compiled from `event:<id>.rhai`. Holds the AST and a cache
/// of which optional functions are present (so the runtime skips calls to
/// missing functions entirely).
pub struct ScriptedEvent {
    /// Stable id (`event:` prefix dropped from filename).
    pub id: String,
    /// Compiled script. Shared across calls.
    pub ast: Arc<AST>,
    /// Path the script was loaded from — for error messages.
    pub source_path: std::path::PathBuf,
    pub has_can_trigger: bool,
    pub has_characters: bool,
    pub has_effect: bool,
    pub has_decline: bool,
}

impl ScriptedEvent {
    /// Compile one event file. Returns the bundle with which optional
    /// functions are present, or an error with the file path.
    pub fn load(engine: &Engine, path: &Path) -> Result<Self> {
        let id = id_from_path(path).with_context(|| {
            format!("event script path `{}`", path.display())
        })?;
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let ast = engine
            .compile(src)
            .with_context(|| format!("compiling {}", path.display()))?;
        let ast_clone = ast.clone();
        let names: Vec<String> = ast_clone
            .iter_functions()
            .map(|f| f.name.to_string())
            .collect();
        let has = |n: &str| names.iter().any(|x| x == n);
        Ok(Self {
            id,
            ast: Arc::new(ast),
            source_path: path.to_path_buf(),
            has_can_trigger: has("can_trigger"),
            has_characters: has("characters"),
            has_effect: has("effect"),
            has_decline: has("decline"),
        })
    }

    /// `title()` — required. Caller treats errors as load-failures.
    pub fn call_title(&self, engine: &Engine) -> Result<String> {
        let mut scope = fresh_scope();
        engine
            .call_fn::<String>(&mut scope, &self.ast, "title", ())
            .with_context(|| format!("event:{}::title", self.id))
    }

    /// `narration()` — required.
    pub fn call_narration(&self, engine: &Engine) -> Result<String> {
        let mut scope = fresh_scope();
        engine
            .call_fn::<String>(&mut scope, &self.ast, "narration", ())
            .with_context(|| format!("event:{}::narration", self.id))
    }

    /// `weight()` — required. Returns `u32` for the draw; the script returns
    /// `i64` because Rhai doesn't distinguish.
    pub fn call_weight(&self, engine: &Engine) -> Result<u32> {
        let mut scope = fresh_scope();
        let n: i64 = engine
            .call_fn(&mut scope, &self.ast, "weight", ())
            .with_context(|| format!("event:{}::weight", self.id))?;
        if n < 0 {
            bail!("event:{}::weight returned negative value {n}", self.id);
        }
        Ok(n as u32)
    }

    /// `choices()` — required. Each element is a record with `text: String`
    /// and an optional `chronicle: String`.
    pub fn call_choices(&self, engine: &Engine) -> Result<Vec<ChoiceRow>> {
        let mut scope = fresh_scope();
        let arr: rhai::Array = engine
            .call_fn(&mut scope, &self.ast, "choices", ())
            .with_context(|| format!("event:{}::choices", self.id))?;
        let mut rows = Vec::with_capacity(arr.len());
        for (i, item) in arr.into_iter().enumerate() {
            let map = item
                .try_cast::<rhai::Map>()
                .with_context(|| format!("event:{}::choices[{i}] is not a map", self.id))?;
            let text = map
                .get("text")
                .and_then(|v| v.clone().into_string().ok())
                .ok_or_else(|| {
                    anyhow::anyhow!("event:{}::choices[{i}] missing `text: String`", self.id)
                })?;
            let chronicle = map
                .get("chronicle")
                .and_then(|v| v.clone().into_string().ok());
            rows.push(ChoiceRow { text, chronicle });
        }
        if rows.is_empty() {
            bail!("event:{}::choices returned an empty array", self.id);
        }
        Ok(rows)
    }

    /// `decline()` — optional. Returns `None` if absent or broken.
    pub fn call_decline(&self, engine: &Engine) -> Option<String> {
        if !self.has_decline {
            return None;
        }
        let mut scope = fresh_scope();
        engine
            .call_fn::<String>(&mut scope, &self.ast, "decline", ())
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// `can_trigger(world) -> bool` — optional. Defaults to `true` when
    /// absent. Errors return `true` (don't lock out the event because of a
    /// broken script).
    pub fn call_can_trigger(&self, engine: &Engine, world: rhai::Map) -> bool {
        if !self.has_can_trigger {
            return true;
        }
        let mut scope = fresh_scope();
        engine
            .call_fn::<bool>(&mut scope, &self.ast, "can_trigger", (world,))
            .unwrap_or_else(|e| {
                eprintln!("event:{}::can_trigger: {e}", self.id);
                true
            })
    }

    /// `characters(world) -> Array` — optional. Defaults to empty (ambient).
    /// Each element is a character view map (same shape as `world.player`).
    /// The script is responsible for any RNG pick (via `world.ctx.rng`).
    /// Errors return empty.
    pub fn call_characters(
        &self,
        engine: &Engine,
        world: rhai::Map,
    ) -> Vec<bevy::prelude::Entity> {
        if !self.has_characters {
            return Vec::new();
        }
        let mut scope = fresh_scope();
        let arr: rhai::Array = engine
            .call_fn(&mut scope, &self.ast, "characters", (world,))
            .unwrap_or_else(|e| {
                eprintln!("event:{}::characters: {e}", self.id);
                Vec::new()
            });
        let mut out = Vec::with_capacity(arr.len());
        for (i, v) in arr.into_iter().enumerate() {
            let Some(m) = v.try_cast::<rhai::Map>() else {
                eprintln!("event:{}::characters[{i}] is not a map", self.id);
                continue;
            };
            let Some(bits) = m.get("entity").and_then(|v| v.as_int().ok()) else {
                eprintln!(
                    "event:{}::characters[{i}] missing `entity` int",
                    self.id
                );
                continue;
            };
            out.push(bevy::prelude::Entity::from_bits(bits as u64));
        }
        out
    }

    /// `effect(world) -> ()` — optional. Defaults to no-op. Errors are
    /// logged but don't propagate.
    pub fn call_effect(&self, engine: &Engine, world: rhai::Map, scope: &mut Scope) {
        if !self.has_effect {
            return;
        }
        if let Err(e) = engine.call_fn::<()>(scope, &self.ast, "effect", (world,)) {
            eprintln!("event:{}::effect: {e}", self.id);
        }
    }
}

/// Build a fresh [`Scope`] with the engine-level pre-defined constants
/// (currently just `None`) so modders can write Rust-style
/// `#{ chronicle: None }` in map literals alongside `Some(...)`. Pairs
/// with the `Some` / `None` function registrations in
/// `script_ctx::register_api`, which cover the `Some(x)` and `None()`
/// call-style; the constant here covers the bare-identifier style.
fn fresh_scope() -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push_constant("None", ());
    scope
}

/// Extract `<id>` from a filename like `event-<id>.rhai`. The `event-` prefix
/// is required. Cross-platform: a colon-prefixed convention (`event:<id>`)
/// would be more conventional on Unix, but Windows treats `:` as a reserved
/// character — the dash keeps filenames portable.
fn id_from_path(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("event script `{}` has no usable filename", path.display()))?;
    let id = stem
        .strip_prefix("event-")
        .ok_or_else(|| {
            anyhow::anyhow!(
                "event script `{stem}` must start with `event-` (e.g. `event-foo.rhai`)"
            )
        })?;
    if id.is_empty() {
        bail!("event script `{}` has empty id", path.display());
    }
    Ok(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_from_filename() {
        let p = Path::new("mods/base/event-foo.rhai");
        assert_eq!(id_from_path(&p).unwrap(), "foo");
        let p = Path::new("event-wayfaring_stranger.rhai");
        assert_eq!(id_from_path(&p).unwrap(), "wayfaring_stranger");
        let p = Path::new("event-.rhai");
        assert!(id_from_path(&p).is_err());
        let p = Path::new("buildings.ron");
        assert!(id_from_path(&p).is_err());
    }
}

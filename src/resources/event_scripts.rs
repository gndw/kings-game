//! The compiled event roster: `Vec<ScriptedEvent>` collected by `mods::load`'s
//! third pass and inserted as a Bevy resource in `main`. Paired with an
//! `rhai::Engine` resource that has the `ScriptCtx` API registered.

use crate::scripted_event::ScriptedEvent;
use bevy::prelude::Resource;
use rhai::Engine;

#[derive(Resource)]
pub struct EventScripts {
    pub engine: Engine,
    pub events: Vec<ScriptedEvent>,
}

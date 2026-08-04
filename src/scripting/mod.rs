//! Scripting integration via `bevy_mod_scripting`.
//!
//! This module wires up multi-language scripting (Lua) into Kings Game. Scripts
//! live as Bevy assets and are attached to the world as static scripts — they
//! receive callbacks via named event labels (`on_month`, `on_day`).
//!
//! Scripts are loaded from `assets/scripts/` and hot-reloaded by Bevy's asset
//! system. The cold-path hooks (monthly, daily) let mods react to game events
//! without touching the hot-loop economy computation.
//!
//! See the analysis at
//! <https://coolness-clawbot.uk/kings-game-bms-modding.html> for the full design.

use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;

/// Callback labels for Kings Game script hooks. Scripts define functions
/// matching these names to react to game events.
///
/// - `on_month` — fired once when the in-game month rolls over.
/// - `on_day` — fired every simulated day (every `FixedUpdate` tick).
callback_labels!(
    OnMonth => "on_month",
    OnDay => "on_day"
);

/// The scripting plugin: adds BMS, loads scripts, and registers event handlers.
///
/// Add this to the App alongside the other plugins. It enables Lua scripting
/// and hooks the game's lifecycle events into script callbacks.
pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BMSPlugin)
            .add_plugins(LuaScriptingPlugin::default())
            // Event handlers dispatch `ScriptCallbackEvent`s to the matching
            // script functions. They run in `Update` so they fire after
            // `FixedUpdate` produces events each tick.
            .add_systems(
                Update,
                (
                    event_handler::<OnMonth, LuaScriptingPlugin>,
                    event_handler::<OnDay, LuaScriptingPlugin>,
                ),
            )
            .add_systems(Startup, load_scripts);
    }
}

/// Load all `.lua` scripts from `assets/scripts/` and attach them as static
/// scripts (not bound to any entity). This runs once at startup.
///
/// Each script file becomes a globally-available script that receives all
/// callback events. Scripts define functions like `on_month()` to react.
fn load_scripts(asset_server: Res<AssetServer>, mut commands: Commands) {
    // BMS loads scripts via the Bevy asset system. Files live under
    // `assets/scripts/` and are discovered by the asset server.
    let script_path = "scripts/economy_overhaul.lua";
    let handle: Handle<ScriptAsset> = asset_server.load(script_path);

    // Attach as a static script — no entity binding, receives all events.
    commands.queue(AttachScript::<LuaScriptingPlugin>::new(
        ScriptAttachment::StaticScript(handle),
    ));

    info!("Scripting: loaded {}", script_path);
}

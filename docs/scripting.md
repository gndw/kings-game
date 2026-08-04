# Scripting

Kings Game uses [`bevy_mod_scripting`](https://github.com/makspll/bevy_mod_scripting) (BMS) for moddable scripted rules.

## Current state

- **Language:** Lua 5.4 (`lua54` feature)
- **Hook points:** `on_day` (every simulated day), `on_month` (on month rollover)
- **Script location:** `assets/scripts/*.lua`
- **Hot reload:** yes — edit the script file and Bevy's asset system reloads it

## Writing a script mod

1. Create a `.lua` file in `assets/scripts/`.

2. Define callback functions for the events you want to handle:

```lua
-- Print a chronicle entry every month
function on_month()
    print("A new month begins")
end

-- Lightweight daily hook (fires every simulated day)
function on_day()
    -- Keep this minimal — daily hooks at scale can be expensive
end
```

3. The script is automatically loaded at startup and receives callbacks.

## ECS access from scripts

Scripts access the ECS world via BMS's reflection-based bindings. Components are accessible through the `world` global:

```lua
-- Read a resource
local date = world:get_resource("Date")
local day = date.day
local month = date.month

-- Query entities
local query = world:query()
    :component("CharacterGold")
    :component("CharacterGoldYield")
```

All Kings Game components are reflected:
- `Character`, `CharacterName`, `CharacterAge`, `CharacterGold`, `CharacterLevy`, `CharacterGoldYield`
- `House`, `HouseName`
- `Land`, `LandName`, `LandBorders`, `LandHolding`, `Built`
- `Kingdom`, `LedBy`, `Seat`, `Holds`, `Leads`, `HeldBy`
- `StringId` (the entity's RON data id)

## Performance constraints

**Cold path only.** Script callbacks fire once per event — not per entity. Do not iterate thousands of entities in script hooks. For per-entity hot-loop computation, use modifier tables (declarative rules evaluated at native speed) or native Rust systems.

See the [BMS analysis](https://coolness-clawbot.uk/kings-game-bms-modding.html) for the full performance breakdown.

## Adding new callback labels

1. Add the label to `callback_labels!` in `src/scripting/mod.rs`.
2. Add an `event_handler::<YourLabel, LuaScriptingPlugin>` system in the plugin's `Update` schedule.
3. Fire the event from game logic using `Messages<ScriptCallbackEvent>`.

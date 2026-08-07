# Architecture

How `kings-game` is put together. An agent should read this before a change that
touches structure (new entity kind, new schedule, new data section, refactoring a
layer), and **update this file when the structure it describes changes**. If a
section stops matching the code, fix the section in the same change.

`docs/decision.md` holds *why* the structure is the way it is; this file holds
*what* it is.

## One-paragraph summary

A Bevy `App` runs three schedules — `Startup`, `FixedUpdate` (the tick), and
`Update` (render + input) — plus one custom `OnMonth`. The world is Bevy ECS
(not hecs, despite the README): every land, character, house, kingdom, courtier and building is an
entity, and a read-only building-definition roster plus the calendar, date and map border
and chronicle log are `Resource`s. Session state (rng, player id, map selection)
lives in a single `Game` resource wrapping `Ctx`. All of the *what exists* comes
from mod folders of RON data, loaded in two passes — definitions merge by id,
then state overlays the mutable fields — and is consumed once by `populate` to
spawn entities, after which the ECS is the whole world.

## Layers

```
        mods/*.ron  (data on disk)
            │  mods::load  (two-pass: defs merge, then state overlay + reconcile)
            ▼
        Content  (one struct per kind, merged definitions + state)
            │  ecs::populate  (once, in main, before App::run)
            ▼
  ┌───────────────────────── Bevy App world ─────────────────────────┐
  │  Entities (Bevy ECS)              Resources                       │
  │   House, Character, Land,          Registry (id→Entity)           │
  │   Kingdom, Building + relations   BuildingDefs (kind roster)      │
  │            + one-field components  Calendar, Date, Border,        │
  │                                    Chronicles, Game(Ctx): rng,   │
  │                                     player id, selection         │
  │                                                                   │
  │  Schedules:  Startup →  FixedUpdate(tick) →  Update(render/input) │
  │              + OnMonth (run from the tick on month rollover)      │
  └───────────────────────────────────────────────────────────────────┘
```

Data flows **down**: files → `Content` → entities + resources, then never flows
back (there is no save-write path yet). The sim reads entities/resources and
mutates entity components in place; the UI reads entities/resources through
Bevy `Query` system params.

## Data pipeline (the load half)

Lives in `mods/`, `content.rs`, `state.rs`, `resources/`.

- **`mods::load(dir)`** (`src/mods/mod.rs`) is the only entry point. It walks
  `mods/` folder by folder in **sorted name order** (later folders override
  earlier), and **two passes** per the content/state contract:
  1. Every `*.ron` that is *not* `*.state.ron` is a **definition** file
     (`content::parse_file` → `ContentFile`), merged by `Content::merge`.
  2. Every `*.state.ron` is a **state** file (`state::parse_file` →
     `StateFile`), overlaid by `Content::merge_state` onto the now-complete
     definitions.
  Between the passes `content::validate` runs (fatal on dangling refs); after,
  `state::reconcile` runs (repairs rather than refuses, returns notes printed
  to stderr — this is the "old save vs new content" resilience).

- **Filename is documentation, not schema.** Every `*.ron` is the same
  optional-everything `ContentFile`; the base game's split into
  `lands.ron`/`buildings.ron`/… is for humans. `*.state.ron` is the one
  exception. RON is parsed with `IMPLICIT_SOME` so modders write
  `border: (...)` not `border: Some((...))`.

- **Definition vs state split (the save contract).**
  - *Definitions* = read-only, only ever grows: map geometry, the building
    catalogue (one entry per kind), houses, who characters are (name/house). Authored by hand, so a dangling
    reference is a **fatal** mod bug.
  - *State* = the mutable half, what a save holds: ages, treasuries, levies,
    yields, what's built, who rules what. An overlay keyed by id; unknown ids are
    silently dropped (no separate map to diff against anymore).
  - Each kind's struct holds **both** halves in one non-`Option` struct
    (`Character`, `Land` in `content.rs`); `merge_state` copies only the state
    fields across so definition data is never clobbered. `Kingdom` is state-only.

- **`Content`** (`content.rs`) is the merged result: `IndexMap`s (id-keyed for
  O(1) lookup, insertion-ordered for deterministic iteration) for lands, houses,
  characters, kingdoms, building instances; a `BuildingDefs` roster (the
  catalogue of building kinds); a `Border`; a `Calendar`. It exists
  only between `load` and `populate`; afterwards the ECS owns everything.

- **`resources/`** are the data shapes that become `Resource`s (not entities):
  `Border`, `Calendar` (+`validate`, carries the starting `Date` too),
  `Date` (the walking clock, seeded from `Calendar::start`),
  `BuildingDefs`/`BuildingDef` (read-only roster of building kinds), and
  `Chronicles` (the append-only chronicle log).

## The ECS world

Lives in `ecs/`. `src/ecs.rs` is the module root (with the canonical map of
"which components each entity kind carries") and re-exports the per-kind modules
flat. `decision.md` is the authoritative *why* for the component/relationship
shape; this is the *what*.

- **Entity kinds** are marker-tag components: `House`, `Character`, `Land`,
  `Kingdom`. Each kind's data is **one field per component** so a system queries
  only what it touches (payout needs gold + yield, not age), in its own file:
  `house.rs`, `character.rs`, `land.rs`, `kingdom.rs`.

- **`StringId`** (`ecs/ecs.rs`): every entity carries the id its RON data and
  saves address it by. The Rhai script ABI was string ids; the `Registry`
  (`id → Entity`, a `Resource`) keeps that O(1) lookup. Standard two-step when
  mutating: pull the cheap `Copy` `Entity` out of the registry, drop the borrow,
  then touch the entity.

- **Bevy-native relationships** (`#[relationship]` / `#[relationship_target]`),
  hook-maintained, no manual reverse insert. Every link is named
  `<Attached-to><Verb-or-preposition><Target>` so the component name tells
  you which entity it sits on:
  - `KingdomLedBy` (on kingdom) ↔ `CharacterLeads` (on leader character) —
    one-to-one. Read the reverse via `CharacterLeads::kingdom()`.
  - `KingdomHold` (on kingdom) ↔ `LandHeldBy` (on land, single `Entity`) — a
    kingdom declares its held land; the land's `LandHeldBy` auto-fills. Read
    via `LandHeldBy::kingdom()`.
  - `BuildingOnLand` (on building) ↔ `LandHasBuildings` (on land,
    `Vec<Entity>`) — a building declares its land; the land's
    `LandHasBuildings` auto-fills. Iterate via `RelationshipTarget::iter`.
  - `CourtierOfCharacter` (courtier→character) ↔ `CharacterHasCourtiers`, and
    `CourtierOfKingdom` (courtier→kingdom) ↔ `KingdomHasCourtiers` — each appointment
    links one character to one kingdom; `CourtierType::Courtier` is the generic role.
  - Plain (non-relationship) entity links: `CharacterOfHouse`
    (character→house), `BuildingOf`
    (building→definition id, a string looked up against the `BuildingDefs`
    resource — not an entity link, since definitions are a roster, not
    entities), `BuildingStatus` (`Active` / `Inactive` / `Building` — only
    `Active` counts toward yield), and on `Building` instances a
    `BuildingConstructionDate` set to start date + def's `construction_time`
    (removed once the per-day tick flips the status). The kingdom's seat is
    implicit: its single held land.

- **`populate(world, content)`** (`ecs/ecs.rs`) builds the world **once** from
  merged+reconciled content, called from `main` before `App::run`. Spawn order is
  **leaves-first** (houses → characters → lands → buildings → kingdoms) so every
  relationship resolves to an entity that already exists. `reconcile` has
  already pruned dangling refs, so the `filter_map`s here guard logic, not bad
  data. The building *definition* roster leaves as the `BuildingDefs` resource;
  each building *instance* becomes an entity related to its land via
  `BuildingOnLand`.

- **Read order is Bevy archetype order**, which within one archetype is spawn
  order. Each kind is a single archetype, so a `Query` over `(&StringId, &Land)`
  yields lands in content order — deterministic, no sort needed.

## Session state & resources

- **`Ctx`** (`ctx.rs`) holds only what isn't an entity: `seed`, the `SimRng`
  behind an `Arc<Mutex<>>`, `player_character_id`, and `selected_land_id` (set
  on `Startup` to the player's own capital via `CharacterLeads`→`KingdomHold::land()`).
  The chronicle log
  is not here — it is the separate `Chronicles` resource. Gold/levy are
  **not** here — every character has their own components and the player is
  only distinguished by the id.
- **`Game`** (`app.rs`) is the `Resource` wrapping `Ctx`, plus `paused` and
  `speed_idx` (index into `Calendar::speeds`, because the rates are mod data).
  `Game::running()` gates the tick.
- The static `Resource`s (`Border`, `Calendar`, `Date`, `BuildingDefs`) and the
  `Chronicles` log are seeded in `main`; `Registry` is seeded by `populate`. The
  command palette's open/active-command/step/cursor state is the `CommandMenu`
  resource (`ui/command_menu.rs`), seeded in `main`; the roster of commands it
  offers is the `CommandRegistry` resource (`commands/core.rs`), also seeded in
  `main`.
- **`ctx::step`** is an exclusive `&mut World` free function: selection movement
  by direction heuristic over land holdings (no adjacency graph — see the
  `ponytail:` note; revisit if picks feel wrong).

## The simulation loop

Lives in `game/` and `schedules.rs`.

- **Schedules** (`schedules.rs`): Bevy's `Startup`/`Update`/`FixedUpdate`, plus
  two custom labels — `OnDay` (per-day building completions) and `OnMonth`
  (monthly payout) — both run from `advance` after the date mutates.
- **The tick — `advance`** (`game/advance_date.rs`) runs in `FixedUpdate`,
  gated by `Game::running()`. It's an **exclusive `fn(&mut World)`** because
  it needs `run_schedule(...)`, which requires `&mut World`. It bumps
  `tick_count`, advances `day`/`month`/`year` against the `Calendar`, then
  runs `OnDay`; on the day the date rolls back to 1 it also runs `OnMonth`.
  `FixedUpdate`'s rate is set by `input` from
  `speed(&calendar.speeds, speed_idx)` — simulated days per real second.
- **The economy is Rust, not a script:**
  - `recompute_yields` (`game/yields.rs`) — runs in `Startup` (so the
    opening screen shows what a realm renders). After that the construct
    and destroy commands trigger a custom [`OnBuildingUpdated`]
    (`game/yields.rs`) event (`kind = 1 = constructed` /
    `3 = destroyed`; `2 = updated` is reserved for future code paths that
    move a building or hot-swap its definition) straight after their
    structural change; its `On<OnBuildingUpdated>` observer walks
    `land → LandHeldBy → kingdom → KingdomLedBy → leader`, runs the shared
    [`sum_kingdom_yield`] helper over
    `kingdom → KingdomHold → land → LandHasBuildings → BuildingOf →
    BuildingDefs`, and writes that one character's [`CharacterGoldYield`]
    and [`CharacterLevy`]. The event fires *after* the relationship hook
    has settled `LandHasBuildings` (construct → hook adds; destroy → hook
    pulls), so `sum_kingdom_yield` always sees authoritative data.
  - `ui::resource::update` runs in `PostUpdate` (one of Bevy's built-in
    schedules, strictly after `Update` finishes), so the bar's read against
    `CharacterGoldYield` happens on the same frame as the event-driven
    write rather than the next. Other UI systems (`map::update_input/draw`,
    `information::update`, `buildings::update`, `chronicle::update`,
    `status::update`, etc.) stay in
    `Update` — the bar is the one that has to react to ECS writes from
    `Update`'s event-driven recompute.
  - `payout` (`game/payout.rs`) — runs in `OnMonth`. Pays every leader
    (entities carrying `CharacterLeads`) their `CharacterGoldYield` into
    `CharacterGold`.
    Signed both places: debt and losses are real, no floor.
- **No Rhai right now.** The README's *Scripts* section describes a Rhai hook
  surface (`on_startup`/`on_day`/`on_month`); it was pulled out during the ECS
  refactoring and `mods/mod.rs` currently ignores `*.rhai` files. Treat the
  README's script tables as the *intended* surface, not the current one.

## Player commands

Lives in `commands/`. The first mutation path driven by player input (prior
input only navigated the selection and set sim speed). A [`Command`] trait type
is *what to do* — and it is **self-describing**: each command owns its rules
(validation), its UI (a fixed run of selection steps that read the world), and
its effect (`execute`). The *who* (a character id) is passed to both
`step_items` and `execute`, so the same path serves the player now and AI /
networked peers later. `execute` is the one `&mut World` touch per command, in
the style of `ctx::step`.

- **Layout:** `commands.rs` (root + re-exports) + `commands/core.rs` (the
  `Command` trait, `MenuItem`/`Choice`, the `CommandRegistry` resource, and the
  shared id/chronicle/ruled-lands helpers) + one submodule per command
  (`construct_building.rs`, `destroy_building.rs`). The input path that *drives*
  a command's steps is the palette in `ui/command_menu.rs` (see UI) — there is no
  key handler in `commands/`.
- **Extending** = a new struct implementing `Command` (its steps + rules +
  effect) + one `register` line in `CommandRegistry::default`. No palette edits:
  the menu is command-agnostic, driving any registered command's steps the same
  way. The registry is a `Resource`, so a plugin/mod could push more before
  `App::run`.
- **Self-describing steps.** A command declares `step_count`; `step_items(step,
  choices, actor, &World)` returns the selectable rows for that step, where later
  steps see the earlier picks in `choices` (e.g. *Destroy Building* lists the
  buildings standing on the land picked at step 0). The menu recomputes the list
  in its exclusive `input` (the only path with `&World`) and stores it on the
  `CommandMenu` resource for the non-exclusive `update` to render.
- **One issuer now (the player); queue deferred.** The command palette builds
  the choices from the player's picks and calls the command's `execute`
  immediately in an exclusive `Update` system. A `CommandQueue` drained per tick
  is the obvious next step if a second issuer arrives (AI, replay, multiplayer);
  not built speculatively.
- **`ConstructBuilding`** validates (def exists in `BuildingDefs`; actor's
  kingdom — via `CharacterLeads` — equals the land's `LandHeldBy`, i.e. they
  rule it; gold ≥ `construction_price`, no debt), then spawns the same bundle
  `populate` uses
  (`StringId`/`Building`/`BuildingOf`/`BuildingOnLand` + `BuildingStatus::Building`
  + `BuildingConstructionDate(start + def.construction_time)`), registers the
  id in `Registry`, deducts gold, and appends a chronicle line on success
  *and* every rejection. The new building contributes no yield yet; the
  per-day `construction` system (`game/construction.rs`) flips it to
  `Active` once the date passes the finish date and fires `OnBuildingUpdated`
  so the realm's yields refresh through the same observer.
- **`DestroyBuilding`** (the inverse) validates the actor rules the land and
  the building is `BuildingOnLand` it, then despawns the instance +
  deregisters its id. Despawning auto-removes it from the land's
  `LandHasBuildings` (the relationship hook); `recompute_yields` drops its
  yield next tick.
- **Runtime building id** is a v4 UUID drawn from the seeded `SimRng` (not OS
  entropy), keeping the one-entropy-source invariant; format-only, no `uuid`
  crate.

## Input

`app::input` (`Update`) handles global keys: `q`/`esc` → `AppExit` (but `esc`
is yielded to the command palette while it is open), `space` → toggle
`Game::paused`, the digit keys jump `speed_idx` through `Calendar::speeds` and
update the `FixedUpdate` timestep. `ui::map::update_input` (exclusive) handles
arrow keys → `ctx::step` → move the selection, but yields the arrows to the
palette while it is open. `ui::command_menu::input` (exclusive) opens the
spotlight-style command palette on `c` and navigates it (arrows + enter +
Esc); the final step's pick hands the accumulated choices to the picked command's `execute`.

## UI

Lives in `ui/`. All Bevy UI (flex `Node` tree) + `Gizmos` line drawing; no
asset-loaded sprites.

- **Layout** (`ui/startup.rs`): a column flex tree — `resource` bar on top, a row
  holding the map (left, full remaining width) and the right-hand column
  (`information` over `courts` over `buildings` over `actions` over `chronicle`, the latter
  pinned to 30% height),
  `status` bar on
  the bottom. `RIGHT_BAR = 0.3` is shared with the camera so the map lands beside
  the column, not under it.
- **Map** (`ui/map.rs`) and **Camera** (`ui/camera.rs`): the map module owns
  the gizmo drawing (world border, land outlines + fills, holdings, the
  selected land's flag, and the per-land yield labels); `startup` there
  spawns one `Text2d` label per land just below the holding circle. The
  camera module owns the `Camera2d` entity and its `update_camera` system:
  `startup` spawns the camera framed on the whole `Border` with an `AutoMin`
  projection so the island never distorts and never pans, attaching
  `CameraView` (current rendered view) + `CameraTween` (in-flight
  `from`/`to`/`t`). `update_draw` draws the world border, each land's outline
  (gizmos draw lines only, so the fill is a **scanline** routine handling
  the map's concave shapes), holdings as circles, the selected land in
  yellow over its neighbours, the player's own holdings tinted green, and a
  waving pennant (`flag.rs`) on the selection. `update_camera` (PostUpdate,
  runs *before* `update_draw`) computes the destination from
  `Game::zoomed` + `selected_land_id` each frame — unzoomed → whole `Border`,
  zoomed → selected land's polygon bbox + `ZOOM_MARGIN`, centred on the bbox
  — and if the destination moved since last frame, restarts the tween with
  `from` = current rendered view (so a re-target mid-transition stays smooth).
  The tween advances `t` over `TRANSITION_DURATION` seconds with a smoothstep
  ease, then writes the lerped `min_width`/`min_height`/`translation` into the
  camera's `Projection::Orthographic` and `Transform`. `Z` toggles
  `Game::zoomed` in `app::input`; arrow keys move the selection in
  `map::update_input`, and the camera follows because `update_camera`
  re-reads `selected_land_id` each frame and re-tweens when it changes.
  Pan/zoom hooks still use the same camera: pan = `Transform::translation`,
  zoom = `OrthographicProjection::scale` (currently constant at
  `CAMERA_SCALE`, 30% zoom-in over a 1:1 view).
- **Panels** each own a marker `Component` (`LegendInfo`, `LegendBuildings`,
  `LegendActions`, `Chronicle`, `ResourceBar`, `Status`) and an `update` system
  that reads the world through `Query`/`Res` and writes into a `Single<&mut Text>`.
  The exceptions are the column *containers* `LegendBuildings` and
  `LegendActions`: their `update` holds a `Single<Entity, With<…>>` and rebuilds
  child rows — buildings only when a `Local` cache key (selection + building
  roster) changes, actions every frame since the list is ≤2 rows.
  - `information` — the selected land, in one panel: a title (`INFORMATION`)
    + a `LegendInfo` text block holding the land name and the ruler
    (name, house, age). Its `update` clears the text on no selection.
  - `buildings` — the selected land, in a sibling panel: a title
    (`BUILDINGS`) + a `LegendBuildings` 3-column table — name (left, fills) /
    gold (right) / levy (right), one row per building, then a thin rule and a
    `total` row in the same layout. Its `update` clears the table on no
    selection.
  - `courts` — courtiers of the kingdom holding the selected land, showing character name and role.
  - `actions` — its own panel between `buildings` and `chronicle`: a title
    (`ACTIONS`) + a `LegendActions` column listing the player's build/destroy
    hotkeys if the player rules the selected land, else a `(none)` placeholder.
    `update` runs as its own system each frame.
  - `chronicle` — last 10 lines of the `Chronicles` resource.
  - `resource` — the player's name, house, gold, yield/mo, levy.
  - `status` — `[PAUSED]`/`[RUNNING]`, the `Date`, current speed, a `C commands`
    hint.
- **Command palette** (`ui/command_menu.rs`): a spotlight-style modal — a
  centered window over a dimmed backdrop, lifted above the panels with
  `GlobalZIndex`. A `CommandMenu` resource holds `open`/active-command/step/
  cursor plus the cached on-screen list and title; `c` opens it, arrows move the
  cursor, `enter` drills into the picked command's own steps, `esc` closes. It is
  **command-agnostic**: it drives *any* registered command's steps the same way.
  The exclusive `input` recomputes the current step's list via the command's
  `step_items` (the one path with `&World`) and stores it on the resource; the
  non-exclusive `update` just renders that stored list, rebuilding rows only when
  `(command, step, cursor)` changes (the buildings panel's cache idea). The final step's
  pick hands the accumulated choices to the command's `execute`. While open it
  owns `esc` and the arrows, so `app::input` and `ui::map::update_input` read its
  `open` flag and yield them.

## Key invariants (things that will bite you if broken)

- **Two-pass load, in order.** State can only overlay entries the definitions
  established; don't merge state before all definitions are in.
- **Leaves-first spawn order** in `populate`. A relationship must resolve to an
  entity that already exists.
- **Every game entity carries a `StringId`**, and `Registry` must be kept in sync
  with spawns. The id is the contract with data, saves, and (someday) scripts.
- **Read order = archetype order = spawn order = content order.** Anything that
  needs stable iteration order relies on each kind being a single archetype.
- **Relationships are hook-maintained.** Set the single-`Entity` side
  (`KingdomLedBy`/`KingdomHold`/`BuildingOnLand`); never hand-edit the reverse
  (`CharacterLeads`/`LandHeldBy`/`LandHasBuildings`).
- **Definition refs are fatal; state refs are repaired.** Don't move
  `validate`'s checks into `reconcile` or vice versa — they encode different
  policies (broken mod vs old save).
- **Determinism.** Sorted load order and the seeded `SimRng` (every draw routed
  through one counter — `rng.rs`) keep saves and replays exact. Don't introduce
  unsorted reads or non-seeded randomness in the sim path.

## File map

| Path | Role |
|---|---|
| `src/main.rs` | arg parse, load mods, build `App`, register systems/schedules, `run` |
| `src/app.rs` | `Game` resource, `Ctx` wrapper, `speed`, `input` |
| `src/commands.rs` | module root + re-exports (`Command`, `CommandRegistry`, `Choice`, `MenuItem`) |
| `src/commands/core.rs` | the `Command` trait, `CommandRegistry`, `MenuItem`/`Choice`, shared helpers (`next_id`, `note`, `ruled_lands`) |
| `src/commands/construct_building.rs` | the `ConstructBuilding` command (validate + spawn as BUILDING + pay) |
| `src/commands/destroy_building.rs` | the `DestroyBuilding` command (validate + despawn + deregister) |
| `src/game/construction.rs` | `tick` — flips `BUILDING` buildings to `ACTIVE` once the date passes their finish date |
| `src/ctx.rs` | `Ctx` (session state: rng, player id, selection), `startup`, selection `step` |
| `src/content.rs` | `Content`, per-kind structs, `parse_file`, `merge`, `validate` |
| `src/state.rs` | `StateFile`, `merge_state`, `reconcile` |
| `src/mods/mod.rs` | `load(dir)` — the two-pass orchestrator |
| `src/resources/*` | `Border`, `Calendar`(+validate, carries `start`), `Date` (the walking clock), `BuildingDefs`/`BuildingDef` (kind roster), `Chronicles` (log) |
| `src/ecs/ecs.rs` | `StringId`, `Registry`, `populate` |
| `src/ecs/{house,character,land,building,kingdom,courtier}.rs` | components + relationships per kind |
| `src/ecs.rs` | module root, re-exports, the component map |
| `src/schedules.rs` | `OnDay` + `OnMonth` labels |
| `src/game/advance_date.rs` | the tick (exclusive `&mut World`) |
| `src/game/yields.rs` | `recompute_yields` (graph walk) |
| `src/game/payout.rs` | `payout` (monthly gold to leaders) |
| `src/rng.rs` | `SimRng` — seeded, draw-counted for exact replay |
| `src/ui/*` | flex layout, map/camera gizmos, the four text panels |
| `src/ui/camera.rs::update_camera` | reads `Game::zoomed` + selection, tweens the camera each PostUpdate |
| `src/ui/command_menu.rs` | the command palette modal (open/navigate/dispatch + render) |

## Related docs

- `docs/decision.md` — *why* the structure is this way (the ECS/world merge, the
  Bevy relationships, the one-field-per-component split, the definition+state
  one-struct decision). Read before restructuring.
- `README.md` — player/modder-facing. Note: it still says "hecs simulates it" and
  documents Rhai scripts that are currently disabled; treat it as the target
  experience, not a map of the current code.

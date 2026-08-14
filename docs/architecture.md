# Architecture

How `kings-game` is put together. Read before a structural change (new entity
kind, new schedule, new data section, refactor across a layer); update this
file in the same change when the structure it describes shifts.

`docs/decision.md` holds *why* the structure is the way it is; this holds
*what* it is.

## One-paragraph summary

A Bevy `App` runs three schedules (`Startup`, `FixedUpdate` tick, `Update`
render + input) plus two custom labels (`OnDay`, `OnMonth`) fired from the
tick. The world is a single Bevy ECS world; every domain object is an entity
and chunky non-entity state (calendar, building roster, border, chronicle log,
session) is a `Resource`. Everything that exists comes from mod RON files
loaded in two passes (definitions merge, then state overlays), then handed
to `populate` which spawns entities — after which the ECS owns the world.

## Layers

```
        mods/*.ron  (data on disk)
            │  mods::load  (two-pass: defs merge, then state overlay + reconcile)
            ▼
        Content  (one struct per kind, merged definitions + state)
            │  ecs::populate  (once, before App::run)
            ▼
  ┌───────────────────────── Bevy App world ─────────────────────────┐
  │  Entities (Bevy ECS)              Resources                       │
  │   marker per kind + one-field    Registry (id → Entity)           │
  │   components + Bevy relationships   BuildingDefs (kind roster)     │
  │                                      Calendar, Date, Border,       │
  │                                      Chronicles, Game(Ctx)         │
  │                                                                   │
  │  Schedules:  Startup →  FixedUpdate(tick) →  Update(render/input) │
  │              + OnDay + OnMonth (run from the tick)                │
  └───────────────────────────────────────────────────────────────────┘
```

Data flows down — files → `Content` → entities + resources — then never
flows back (no save-write path yet).

## Data pipeline

Entry point `mods::load(dir)`. Walks `mods/` in sorted folder order, two
passes:

1. **Definitions** — every `*.ron` that isn't `*.state.ron` merges into
   `Content` (id-replace; later folders override earlier).
2. **State** — every `*.state.ron` overlays the mutable half of the same
   structs field by field (`merge_state`).

`validate` runs between passes (fatal on dangling refs — content is
authored); `reconcile` runs after (repairs, drops notes — state is a save).
Definition refs are fatal; state refs are repaired. Don't mix the two.

Every `*.ron` parses to the same optional-everywhere `ContentFile` —
filename is human organisation, not schema. `IMPLICIT_SOME` so modders
write `border: (...)` not `border: Some(...)`.

The two halves share one struct per kind so `populate` reads everything
off one place. `Kingdom` is state-only.

## The ECS world

- **One marker tag per entity kind** (`House`, `Character`, `Land`,
  `Kingdom`, `Building`, `Army`, `Marching`, `Road`, `War`, `Siege`,
  `Courtier`); data is one field per component so a system queries only
  what it touches.
- **Bevy-native relationships** (`#[relationship]` / `#[relationship_target]`)
  for every link. Naming convention: `<On-entity><Verb-or-preposition><Target>`
  so the component name tells you which entity it sits on. Set the
  single-`Entity` side; never hand-edit the reverse — Bevy's hook keeps it
  in sync.
- **Reverse Vecs are queues/collections where insertion order matters**
  (e.g. the army's marching queue). Otherwise the relationship is single
  `Entity` with a public accessor.
- **Plain (non-relationship) components** for static links (e.g. a
  building's def id, a road's polyline baked at populate time). Use a
  relationship only when the link is dynamic.
- **`StringId` on every entity**, `Registry` (`id → Entity`) on the
  world. The two-step lookup is the contract with data, saves, and
  scripts — pull the `Copy` `Entity` out, drop the borrow, then mutate.
- **`populate(world, content)`** runs once in `main` before `App::run`.
  Spawn order is **leaves-first** so every relationship resolves to an
  existing entity. `reconcile` has already pruned dangling refs;
  `filter_map`s here guard logic, not bad data.
- **Read order = archetype order = spawn order = content order.** Each
  kind is a single archetype, so `Query` yields deterministic order
  without sorting.

## Session state & resources

- **`Ctx`** — only what isn't an entity: seed, `SimRng` behind an
  `Arc<Mutex<>>`, `player_character_id`, `selected_land_id`.
- **`Game`** — `Resource` wrapping `Ctx`, plus `paused`, `speed_idx`,
  `zoomed`. `Game::running()` gates the tick.
- **`Registry`**, **`Border`**, **`Calendar`**, **`Date`**, **`BuildingDefs`**,
  **`Chronicles`** — seeded in `main`; the latter is read-only for
  game-logic code (events observed in `chronicles.rs` write it).
- **`CommandMenu`**, **`CommandRegistry`/`CommandContext`** — UI state
  for the palette and the roster of registered commands; seeded in `main`.

## The simulation loop

- **Schedules** — Bevy's `Startup` / `Update` / `FixedUpdate`, plus two
  custom labels (`OnDay`, `OnMonth`) run from `advance` after the date
  mutates.
- **`advance`** is an exclusive `fn(&mut World)` in `FixedUpdate`, gated
  by `Game::running()`. Bumps `day`/`month`/`year`, then runs `OnDay`; on
  month rollover also runs `OnMonth`. `FixedUpdate` rate is set by `input`
  from `Calendar::speeds[speed_idx]`.
- **The economy is Rust, not a script.** `OnBuildingUpdated` event fires
  on construct/destroy/raise/dismiss; an observer walks the realm's
  holdings and recomputes the leader's yield + levy. `payout` runs in
  `OnMonth` and pays every leader their yield into gold. Debt is real
  (signed).
- **No Rhai right now.** The README's script tables describe the
  intended surface; `mods/mod.rs` ignores `*.rhai` files.

## Player commands

One trait, one registry, one palette. Each command is a struct
implementing `Command`/`BaseCommand` that owns its rules (validation),
its UI (a fixed run of selection steps reading the world), and its
effect (`execute`). The palette drives any registered command's steps
the same way — the menu is command-agnostic.

- **Layout** — `commands.rs` (root) + `commands/core.rs` (trait,
  registry, shared helpers) + one submodule per command. No key
  handler in `commands/`; the palette drives the flow.
- **Extending** = new struct implementing the trait + one `register`
  line. No palette edits.
- **Self-describing steps** — `step_count` + `step_items(step, choices,
  actor, &World)` returns the rows for that step; later steps see
  earlier picks in `choices`. The palette's exclusive `input`
  recomputes the list (the one path with `&World`) and stores it on
  the resource for the non-exclusive `update` to render.
- **One issuer now (the player); queue deferred.** `execute` runs
  immediately in the palette's exclusive `Update`. A `CommandQueue`
  drained per tick is the next step if a second issuer arrives
  (AI/replay/multiplayer).

## Input

`app::input` (Update) — global keys (`q`/`esc` exit, `space` pause,
digit speed, `Z` zoom); yields to the palette while it's open.
`ui::map::update_input` (exclusive) — arrow keys move the selection,
yields to the palette. `ui::command_menu::input` (exclusive) — `c`
opens the palette, drives the active command's steps. `ui::wiki::input`
(exclusive) — `w` toggles the wiki; arrow keys navigate the house list,
`Enter` drills into a house's family tree, `Esc` backs out. `ui::error::input`
(Update) — `esc` closes the error popup, gated to the popup layer.

## Chronicle generation

One observer module, one observer per event. The chronicle is the
*story* — past tense, third person, names lands and armies, never
ids or game-mechanic words. Commands and ticks only `world.trigger(...)`;
the module reads display names off the world and writes one line to
`Chronicles`. Future mod-voicing is a one-file change.

## UI

Bevy flex tree + `Gizmos` line drawing; no asset sprites.

- **Layout** — `ui/startup.rs` builds a column flex: `resource` bar
  top, row of map (left, full remaining width) + right column
  (`information` / `courts` / `buildings` / `chronicle`), `status` bar
  bottom. `RIGHT_BAR = 0.3` is shared with the camera so the map lands
  beside the column, not under it.
- **Panels** — each owns a marker `Component` and an `update` system
  that reads through `Query`/`Res` and writes a `Single<&mut Text>`.
  An empty panel collapses out via `Display::None`.
- **Map** — `ui/map.rs` owns arrow-key selection; the per-component
  graphics (`border_graphic`, `land_graphic`, `holding_icon`,
  `road_graphic`) own the paint. `ui/camera.rs` owns the `Camera2d`
  and tweens between whole-map and zoomed-on-selection views.
- **Command palette** — spotlight modal over a dimmed backdrop,
  `GlobalZIndex` above the panels. `CommandMenu` resource holds
  open/active-command/step/cursor + the cached list + search query.
  Command-agnostic — drives any registered command's steps.
- **Wiki window** — a `W`-toggled modal panel (`ui::wiki`); `Esc`
  closes it. An `InputLayer::Wiki` gates root-layer keys while it's
  open.

## Key invariants

- **Two-pass load, in order.** State can only overlay entries the
  definitions established.
- **Leaves-first spawn order** in `populate`. A relationship must
  resolve to an entity that already exists.
- **Every game entity carries a `StringId`**, and `Registry` is kept
  in sync with spawns.
- **Read order = archetype order = spawn order = content order.**
- **Relationships are hook-maintained.** Set the source side; never
  hand-edit the reverse.
- **Definition refs are fatal; state refs are repaired.** Don't move
  `validate`'s checks into `reconcile` or vice versa.
- **Determinism.** Sorted load order and the seeded `SimRng` (every
  draw routed through one counter) keep saves and replays exact.

## File map

| Path | Role |
|---|---|
| `src/main.rs` | arg parse, load mods, build `App`, register systems/schedules, `run` |
| `src/app.rs` | `Game` resource, `Ctx` wrapper, `speed`, `input` |
| `src/ctx.rs` | `Ctx` (session state), `startup`, selection `step` |
| `src/content.rs` | `Content`, per-kind structs, `parse_file`, `merge`, `validate` |
| `src/state.rs` | `StateFile`, `merge_state`, `reconcile` |
| `src/mods/mod.rs` | `load(dir)` — the two-pass orchestrator |
| `src/resources/` | `Border`, `Calendar`(+validate, `start`), `Date`, `BuildingDefs`, `Chronicles` |
| `src/ecs/ecs.rs` | `StringId`, `Registry`, `populate` |
| `src/ecs/*.rs` | marker + components + relationships per entity kind |
| `src/commands/` | the `Command` trait + `CommandRegistry` + one submodule per command |
| `src/chronicles.rs` | chronicle generation — one observer per game event |
| `src/game/` | per-day / per-month ticks — gerund-named systems (aging, advancing_date, besieging, building_releasing, constructing, court_releasing, marching, paying_out, raising_army, replenishing_levy, yielding) |
| `src/schedules.rs` | `OnDay` + `OnMonth` labels |
| `src/events.rs` | the event surface observers and triggers fire |
| `src/rng.rs` | `SimRng` — seeded, draw-counted for exact replay |
| `src/ui/` | flex layout, map/camera gizmos, panels, command palette, error popup |
| `src/map/components/` | per-entity graphics (border, land, road, holding) |

## Related docs

- `docs/decision.md` — *why* the structure is the way it is. Read
  before restructuring.
- `README.md` — player/modder-facing. Note: it still mentions hecs and
  Rhai scripts that are currently disabled; treat it as the target
  experience, not a map of the current code.

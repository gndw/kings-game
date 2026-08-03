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
(not hecs, despite the README): every land, character, house and kingdom is an
entity, and a read-only building roster plus the calendar, date and map border
are `Resource`s. Session state (rng, chronicle log, player id, map selection)
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
  │   Kingdom   + relationships        Buildings (read-only roster)   │
  │            + one-field components  Calendar, Date, Border         │
  │                                    Game(Ctx): rng, chronicle,     │
  │                                     player id, selection          │
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
  - *Definitions* = read-only, only ever grows: map geometry, building roster,
    houses, who characters are (name/house). Authored by hand, so a dangling
    reference is a **fatal** mod bug.
  - *State* = the mutable half, what a save holds: ages, treasuries, levies,
    yields, what's built, who rules what. An overlay keyed by id; unknown ids are
    silently dropped (no separate map to diff against anymore).
  - Each kind's struct holds **both** halves in one non-`Option` struct
    (`Character`, `Land` in `content.rs`); `merge_state` copies only the state
    fields across so definition data is never clobbered. `Kingdom` is state-only.

- **`Content`** (`content.rs`) is the merged result: `IndexMap`s (id-keyed for
  O(1) lookup, insertion-ordered for deterministic iteration) for lands, houses,
  characters, kingdoms; a `Buildings` roster; a `Border`; a `Calendar`. It exists
  only between `load` and `populate`; afterwards the ECS owns everything.

- **`resources/`** are the data shapes that become `Resource`s (not entities):
  `Border`, `Calendar` (+`validate`), `Date` (the walking clock),
  `Buildings`/`Building` (read-only roster).

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
  hook-maintained, no manual reverse insert:
  - `LedBy` (on kingdom) ↔ `Leads` (on leader character) — one-to-one. Read the
    reverse via `Leads::kingdom()`.
  - `HeldBy` (on land) ↔ `Holds` (on kingdom, `Vec<Entity>`) — a land declares
    its kingdom; the kingdom's `Holds` auto-fills. Iterate via
    `RelationshipTarget::iter`.
  - Plain (non-relationship) entity links: `HouseOf` (character→house), `Seat`
    (kingdom→capital land), `Built` (land→`Vec<building_id>` strings, looked up
    against the `Buildings` resource).

- **`populate(world, content)`** (`ecs/ecs.rs`) builds the world **once** from
  merged+reconciled content, called from `main` before `App::run`. Spawn order is
  **leaves-first** (houses → characters → lands → kingdoms) so every relationship
  resolves to an entity that already exists. `reconcile` has already pruned
  dangling refs, so the `filter_map`s here guard logic, not bad data. The
  building roster leaves as the `Buildings` resource, not entities.

- **Read order is Bevy archetype order**, which within one archetype is spawn
  order. Each kind is a single archetype, so a `Query` over `(&StringId, &Land)`
  yields lands in content order — deterministic, no sort needed.

## Session state & resources

- **`Ctx`** (`ctx.rs`) holds only what isn't an entity: `seed`, the `SimRng`
  behind an `Arc<Mutex<>>`, the `chronicles: Vec<String>` log,
  `player_character_id`, and `selected_land_id` (set on `Startup` to the player's
  own seat via `Leads`→`Seat`). Gold/levy are **not** here — every character has
  their own components and the player is only distinguished by the id.
- **`Game`** (`app.rs`) is the `Resource` wrapping `Ctx`, plus `paused` and
  `speed_idx` (index into `Calendar::speeds`, because the rates are mod data).
  `Game::running()` gates the tick.
- The static `Resource`s (`Border`, `Calendar`, `Date`, `Buildings`) are seeded
  in `main`; `Registry` is seeded by `populate`.
- **`ctx::step`** is an exclusive `&mut World` free function: selection movement
  by direction heuristic over land holdings (no adjacency graph — see the
  `ponytail:` note; revisit if picks feel wrong).

## The simulation loop

Lives in `updates/` and `schedules.rs`.

- **Schedules** (`schedules.rs`): Bevy's `Startup`/`Update`/`FixedUpdate`, plus
  one custom `OnMonth` (`ScheduleLabel`), run from the tick.
- **The tick — `advance`** (`updates/advance_date.rs`) runs in `FixedUpdate`,
  gated by `Game::running()`. It's an **exclusive `fn(&mut World)`** because it
  needs `run_schedule(OnMonth)`, which requires `&mut World`. It bumps
  `tick_count`, advances `day`/`month`/`year` against the `Calendar`, and on the
  day the date rolls back to 1 it runs `OnMonth`. `FixedUpdate`'s rate is set by
  `input` from `speed(&calendar.speeds, speed_idx)` — simulated days per real
  second.
- **The economy is Rust, not a script:**
  - `recompute_yields` (`updates/yields.rs`) — runs in `Startup` (so the opening
    screen shows what a realm renders) and `FixedUpdate`. One pass over the
    relationship graph per character: `character → Leads → kingdom → Holds →
    lands → Built → Buildings`, summing `gold_profit - gold_upkeep` into
    `CharacterGoldYield` and `levy` into `CharacterLevy`. `Option<&Leads>` walks
    every character so a non-ruler is zeroed, not left stale.
  - `payout` (`updates/payout.rs`) — runs in `OnMonth`. Pays every leader
    (entities carrying `Leads`) their `CharacterGoldYield` into `CharacterGold`.
    Signed both places: debt and losses are real, no floor.
- **No Rhai right now.** The README's *Scripts* section describes a Rhai hook
  surface (`on_startup`/`on_day`/`on_month`); it was pulled out during the ECS
  refactoring and `mods/mod.rs` currently ignores `*.rhai` files. Treat the
  README's script tables as the *intended* surface, not the current one.

## Input

`app::input` (`Update`) handles global keys: `q`/`esc` → `AppExit`, `space` →
toggle `Game::paused`, `+`/`-` → step `speed_idx` through `Calendar::speeds`
(clamped) and update the `FixedUpdate` timestep. `ui::map::update_input`
(exclusive) handles arrow keys → `ctx::step` → move the selection.

## UI

Lives in `ui/`. All Bevy UI (flex `Node` tree) + `Gizmos` line drawing; no
asset-loaded sprites.

- **Layout** (`ui/startup.rs`): a column flex tree — `resource` bar on top, a row
  holding the map (left, full remaining width) and the right-hand column
  (`legend` over `chronicle`, the latter pinned to 30% height), `status` bar on
  the bottom. `RIGHT_BAR = 0.3` is shared with the camera so the map lands beside
  the column, not under it.
- **Map** (`ui/map.rs`): camera (`Startup`) framed on the whole `Border` with an
  `AutoMin` projection so the island never distorts and never pans. `update_draw`
  draws the world border, each land's outline (gizmos draw lines only, so the
  fill is a **scanline** routine handling the map's concave shapes), holdings as
  circles, the selected land in yellow over its neighbours, the player's own
  holdings tinted green, and a waving pennant (`flag.rs`) on the selection.
- **Panels** each own a marker `Component` (`LegendInfo`, `LegendBuildings`,
  `Chronicle`, `ResourceBar`, `Status`) and an `update` system that reads the
  world through `Query`/`Res` and writes into a `Single<&mut Text>`:
  - `legend` — the selected land, split into two sections separated by a thin
    divider node: section 1 (`LegendInfo`) holds id/name and the holder kingdom
    + ruler (name, house, age); section 2 (`LegendBuildings`) holds the
    per-building and total gold/levy.
  - `chronicle` — last 30 lines of `ctx.chronicles`.
  - `resource` — the player's name, house, gold, yield/mo, levy.
  - `status` — `[PAUSED]`/`[RUNNING]`, the `Date`, current speed.

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
  (`LedBy`/`HeldBy`); never hand-edit the reverse (`Leads`/`Holds`).
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
| `src/ctx.rs` | `Ctx` (session state), `startup`, selection `step` |
| `src/content.rs` | `Content`, per-kind structs, `parse_file`, `merge`, `validate` |
| `src/state.rs` | `StateFile`, `merge_state`, `reconcile` |
| `src/mods/mod.rs` | `load(dir)` — the two-pass orchestrator |
| `src/resources/*` | `Border`, `Calendar`(+validate), `Date`, `Buildings`/`Building` |
| `src/ecs/ecs.rs` | `StringId`, `Registry`, `populate` |
| `src/ecs/{house,character,land,kingdom}.rs` | components + relationships per kind |
| `src/ecs.rs` | module root, re-exports, the component map |
| `src/schedules.rs` | `OnMonth` label |
| `src/updates/advance_date.rs` | the tick (exclusive `&mut World`) |
| `src/updates/yields.rs` | `recompute_yields` (graph walk) |
| `src/updates/payout.rs` | `payout` (monthly gold to leaders) |
| `src/rng.rs` | `SimRng` — seeded, draw-counted for exact replay |
| `src/ui/*` | flex layout, map/camera gizmos, the four text panels |

## Related docs

- `docs/decision.md` — *why* the structure is this way (the ECS/world merge, the
  Bevy relationships, the one-field-per-component split, the definition+state
  one-struct decision). Read before restructuring.
- `README.md` — player/modder-facing. Note: it still says "hecs simulates it" and
  documents Rhai scripts that are currently disabled; treat it as the target
  experience, not a map of the current code.

# Decisions

Standing decisions for this project. Check here before designing anything;
append a new section when a decision is made.

## Single ECS world (no nested `Ctx.world`)

The simulation entities live directly in Bevy's App world. `Ctx` holds only
session state (rng, `player_character_id`, `selected_land_id`); the chronicle
log lives in its own `Chronicles` resource; `Game` wraps `Ctx` as a `Resource`.

- **Why:** Bevy 0.19's `World::query()` needs `&mut World`, so `Query` is only
  friction-free as a *system param*. Keeping a nested `Ctx.world` forced the
  hand-rolled `EntityIndex` + `&self`-reader machinery that `World::query`'s
  `&mut` requirement existed to avoid. Merging lets reads use `Query` directly.
- **Read shape:** UI systems take system-param `Query`/`Res` and read inline.
  Sim logic (`recompute`, `payout`, `step`, selection stepping) lives as
  `&mut World` free functions run from *exclusive* systems (`fn(&mut World)`),
  because it mixes component mutation with resource reads — the one case
  `&mut World` (phased access) handles cleanly. `resource_scope` bridges a
  resource borrow and the world where needed (`monthly_payout`).
- **Ordering:** read order is Bevy archetype order, which within one archetype is
  spawn order; each kind (houses/characters/lands/kingdoms) is a single
  archetype, so `Query` yields content order.
- **`Registry` stays:** the Rhai ABI is string ids, so `id → Entity` lookup is
  still needed; `Registry` is a resource on the App world.
- **Deleted:** `EntityIndex`, the read-model snapshot structs
  (`LandData`/`BuildingData`/… — the UI reads directly now), and the O(n)
  kingdom scans (replaced by the auto-maintained `CharacterLeads` component for
  O(1) character→kingdom lookup).

## Character↔kingdom leader link is Bevy-native (`KingdomLedBy`/`CharacterLeads`)

The kingdom→leader link is a Bevy `#[relationship]` component `KingdomLedBy`
(on the kingdom, single `Entity`, source of truth) paired with the
auto-maintained `#[relationship_target]` `CharacterLeads` (on the leader
character). Inserting `KingdomLedBy` on a kingdom has Bevy's hook keep
`CharacterLeads` on the leader in sync — no manual reverse insert, no drift.

- **One-to-one** (the target holds a single `Entity`): a character leads at
  most one kingdom; if a second kingdom claims the same leader, Bevy drops the
  older `KingdomLedBy`.
- **Naming:** `<Attached-to><Verb-or-preposition><Target>`, so the name tells
  you which entity the component sits on. `KingdomLedBy` puts the verb after
  the kingdom (mirroring Bevy's `LikedBy`); `CharacterLeads` puts the verb after
  the character. The manual `KingdomLedBy` reverse component is gone.

## Kingdom↔lands link is Bevy-native (`KingdomHold`/`LandHeldBy`)

The kingdom→holdings link is a Bevy `#[relationship]` component `KingdomHold`
(on the kingdom, single `Entity`, source of truth) paired with the
auto-maintained `#[relationship_target]` `LandHeldBy` (single `Entity`) on the
held land. A kingdom declares its held land; Bevy's hook keeps the land's
`LandHeldBy` in sync — no manual reverse insert, no drift.

- **Naming:** same `<Attached-to><Verb-or-preposition><Target>` rule as the
  leader link — `KingdomHold` (kingdom, `pub Entity`) / `LandHeldBy`
  (land, single `Entity`). Read via `LandHeldBy::kingdom()`. The target
  field is private (Bevy's `RelationshipTarget` correctness check requires
  it) with a public accessor — same pattern as `CharacterLeads::kingdom()`.

## ECS components split to one field each

`House`/`Character`/`Land` are marker tags; their data is one field per
component: `HouseName`; `CharacterName`, `CharacterAge`, `CharacterGold`,
`CharacterLevy`, `CharacterGoldYield`; `LandName`, `LandBorders`,
`LandHolding`. The old multi-field `ecs::CharacterState` is dissolved into
the four character components above.

- **Why:** smallest-form components let a system query only the field it
  touches (a payout needs gold + yield, not age), and keep each mutable value
  in its own component so Bevy tracks them independently. The marker tags
  (`House`/`Character`/`Land`) still answer "what kind of entity is this".
- **`state`/`content` merged:** superseded by the decision below — `Character`
  and `Land` now each hold definition *and* state fields in one struct, and
  `populate` reads them directly. See "Definition + state: one struct per kind".

## Definition + state: one struct per kind

`CharacterState`/`LandState` are gone. Each entity kind now has a single struct
in `content.rs` that holds *both* its definition fields (name, `house_id`,
geometry) and its state fields (age, treasury, levy, yield). `Kingdom`
(state-only) moved into `content.rs` alongside them.

- **Load is two-pass** (`mods::load`): every definition `*.ron` merges first
  (`Content::merge`, id-replace), then every `*.state.ron` overlays
  (`Content::merge_state`, field-by-field). Two-pass so state can only fill
  entries the definitions established — content is the source of truth for what
  exists.
- **Overlay never clobbers definition data.** `merge_state` copies only the
  state fields onto the matching content entry; `name`/`house_id`/geometry are
  untouched, so a state entry may carry only its state fields. Because the two
  field sets are disjoint, a single non-`Option` struct suffices — no `Option`
  overlay gymnastics, no parallel `CharacterState`.
- **`State` (the parallel map) is gone.** `Content` carries `kingdoms` too;
`reconcile(&mut Content)` repairs building-instance refs and kingdom refs in place.
`populate(world, content)` takes the one struct and reads every field off it.
- **Dropped:** the old "dropped state for unknown …" chronicle notes. With state
  folded into content there is no separate state map to diff against; an unknown
  id is simply never overlaid. Revisit when save files exist.

## Buildings are ECS entities (definitions stay a roster)

Each *built building* is its own entity, not a string id on the land. The
read-only *definition* per building kind stays a resource roster
(`BuildingDefs`/`BuildingDef`, renamed from `Buildings`/`Building` and moved to
`building_definitions.ron`); a building *instance* entity carries
`BuildingOf(def_id)` to reach its stats and a `BuildingOnLand` relationship to
its land.

- **Why entities, not the old `Built(Vec<String>)`:** the task asked for
  buildings to be addressable ECS entities with a relationship to the land, so
  construction/destruction, per-instance state (health, level, …), and scripting
  by instance id all have somewhere to live. Keeping `Built` alongside would be
  duplicated state — the architecture's single-source-of-truth rule rules that
  out, so `Built` is removed and the land's `LandHasBuildings` target (the
  auto-maintained reverse of `BuildingOnLand`) replaces it.
- **Definition roster stays a resource.** Stats (profit, upkeep, levy,
  `construction_price`) are shared and read-only across every instance of a
  kind; copying them onto each entity would duplicate definition data and break
  the definition/state split. So a building entity holds the *def id* and looks
  the stats up, exactly as `Built`'s ids once did — just from the entity side.
- **Direction mirrors `LandHeldBy`/`KingdomHold`:** the building is the child
  declaring its land (`BuildingOnLand`, single `Entity`, source of truth); the
  land's `LandHasBuildings` auto-fills. Same active/passive mirror as the
  land↔kingdom link.
- **Instances are state-only,** like `Kingdom`: a `buildings:` section in
  `*.state.ron` is an id-keyed overlay (`merge_state` id-replaces, since a save
  holds the full set of what's built), and `reconcile` drops any instance whose
  `def_id` or `land_id` no longer resolves — the same repair-not-refuse policy.
- **Spawn order** gains a buildings step after lands and before kingdoms, so
  `BuildingOnLand` resolves to an entity that already exists.

## Player commands are self-describing (`Command` trait + `CommandRegistry`)

Each player command is a struct implementing a `Command` trait that owns its
rules (validation), its UI (a fixed run of selection steps reading the world),
and its effect (`execute`). The command palette drives *any* registered
command's steps the same way; the roster of commands it offers is the
`CommandRegistry` resource.

- **Why a trait + registry, not the earlier `Command` enum + `apply` dispatch:**
  the enum/dispatch model hardwired the palette to one command's flow
  (`Commands`→`Lands`→`Buildings`); adding a second command (e.g. *Destroy
  Building*) would have forked the `Stage` enum and the palette's
  navigate/render into per-command branches. A self-describing trait lets each
  command define its own steps, and the palette stays generic — the menu code is
  unchanged by new commands. This overturns the prior "no trait, no registry"
  note in `commands.rs`: those now earn their keep, because the palette is
  generic over commands and the registry is the natural seam for a plugin/mod to
  register more before `App::run`.
- **`step_items` takes `&World`, not `&mut World`:** it is a read, and keeping
  it immutable lets the menu recompute the list from a shared borrow. The
  helpers (`ruled_lands`, `buildings_on_land`) therefore walk the relationship
  targets (`KingdomHold`, `LandHasBuildings`) via `World::get` rather than
  `World::query`
  (which needs `&mut World`).
- **`Arc<dyn Command>` in the registry:** so the palette can hand a command to
  `execute` (which needs `&mut World`) without holding the registry's borrow —
  clone the `Arc`, drop the borrow, then mutate. Commands are immutable
  definitions, so shared ownership via `Arc` is natural.
- **List computed in the exclusive `input`, rendered by the non-exclusive
  `update`:** `step_items` needs `&World`, which only an exclusive system gets.
  So `input` recomputes the current list into the `CommandMenu` resource;
  `update` reads that stored list (it can't take `&World`). `input`→`update` is
  chained so the just-opened list shows the same frame.

## Camera mode is a boolean (`Game::zoomed`) with a tween between views

The camera has two views — the whole map and "zoomed in on the selected land" —
and toggles between them with `Z`. It is a plain `bool` on `Game`, not a state
machine: `true` means "frame on the selection's bbox with `ZOOM_MARGIN`",
`false` means "frame on `Border`". `ui::camera::update_camera` reads the flag and
the current `selected_land_id` every PostUpdate frame and rewrites the
camera's `Projection::Orthographic { scaling_mode, scale, viewport_origin }`
and `Transform::translation` in place.

- **Why a flag, not an enum / state machine:** two states with one transition
  (toggle) and one extra input (selection id, already on `Ctx`); anything more
  is over-engineering. The flag lives on `Game` because it's session state,
  not UI state — same neighbourhood as `paused`/`speed_idx`.
- **One frame, one write, every frame.** Recomputing a polygon's bbox is
  trivial, so `update_camera` redoes the math each PostUpdate rather than
  diffing `(mode, selected_land_id)`. The selection-following behaviour comes
  for free: when arrow keys move the selection in `update_input`, the next
  `update_camera` reads the new id and re-centres. Caching would be premature
  optimisation.
- **Fit via `AutoMin`, not by hand.** The same `ScalingMode::AutoMin` the
  default view uses — set `min_width = land_w * ZOOM_MARGIN / (1 - RIGHT_BAR)`,
  `min_height = land_h * ZOOM_MARGIN`, translation = bbox centre. Keeps the
  aspect-ratio guarantees the default relies on and the same 30%-zoom-in
  (`CAMERA_SCALE = 0.7`) so the transition doesn't pop. The hand-rolled
  alternative (`scale` → `1.0`, manual world↔screen math) would duplicate the
  AutoMin logic.
- **Z is yielded to the command palette while it's open**, same as `Esc`. The
  palette owns all input while up; `app::input` reads `menu.open` and skips the
  toggle. Listed alongside `C commands` / `days/s` in the status bar.
- **Smoothstep tween between destinations.** Two extra components on the
  camera entity: `CameraView` (last applied view, doubles as "where are we
  now") and `CameraTween { from, to, t }`. Each frame, `update_camera`:
  (1) computes the destination from `(zoomed, selection)`; (2) if the
  destination moved, copies the current `CameraView` into `from`, sets
  `to = target`, resets `t = 0`; (3) advances `t` by `dt / TRANSITION_DURATION`
  (clamped to 1) and applies a smoothstep ease; (4) writes the lerped
  `min_w`/`min_h`/`translation` into the camera. Mid-transition re-targets
  start the new tween from the current rendered view, so the camera never
  jumps. `TRANSITION_DURATION = 0.2s` — snappy on toggle, no strobing on pan.
- **Why a tween, not exponential smoothing:** a tween settles exactly to the
  destination (`t = 1` ⇒ `view = to`), which keeps the on-screen state clean
  for any later code that wants to read it. Exponential smoothing approaches
  asymptotically and would need a snap threshold plus fiddly `dt * rate`
  constants per field. The tween is also ~25 lines; the smoothing version is
  not noticeably shorter.

## A kingdom holds exactly one land

`Kingdom::land_ids: Vec<String>` is gone; the field is `land_id: String`. One
kingdom rules exactly one land, by id. The Bevy relationship was also flipped
from `KingdomHolds(Vec<Entity>)` (the auto-maintained reverse of each land's
`LandHeldBy`) to `KingdomHold(Entity)` — single-entity on both sides.

- **Why data first:** the gameplay we're actually modelling — one ruler over
  one territory at a time, with war being the way a ruler gains a new land —
  is a 1:1 model. The Vec carried no gameplay the 1:1 doesn't (a kingdom
  couldn't *act* across its lands any differently), and the multi-land
  reconcile was paying complexity for an empty abstraction.
- **Why flip the Bevy shape too:** once the data side is 1:1, the runtime Vec
  is paying the same empty-abstraction tax (`RelationshipTarget::iter` over
  one entity, callers guarding `.get(...).ok()` on a Vec that always has 1
  element). The relationship target is private (Bevy's correctness check
  requires it) with a public `KingdomHold::land()` accessor — the same
  pattern `CharacterLeads::kingdom()` uses.
- **Data invariants tightened.** With one land per kingdom, the held land
  is by definition the seat — `seat_land_id` was dropped from the schema and
  the `KingdomSeat` component was removed; the held land is read through
  `KingdomHold::land()`.


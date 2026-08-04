# Decisions

Standing decisions for this project. Check here before designing anything;
append a new section when a decision is made.

## Single ECS world (no nested `Ctx.world`)

The simulation entities live directly in Bevy's App world. `Ctx` holds only
session state (rng, chronicles, `player_character_id`, `selected_land_id`);
`Game` wraps it as a `Resource`.

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
  kingdom scans (replaced by the auto-maintained `Leads` component for O(1)
  character→kingdom lookup).

## Character↔kingdom leader link is Bevy-native (`LedBy`/`Leads`)

The kingdom→leader link is a Bevy `#[relationship]` component `LedBy` (on the
kingdom, single `Entity`, source of truth) paired with the auto-maintained
`#[relationship_target]` `Leads` (on the leader character). Inserting `LedBy`
on a kingdom has Bevy's hook keep `Leads` on the leader in sync — no manual
reverse insert, no drift.

- **One-to-one** (the target holds a single `Entity`): a character leads at
  most one kingdom; if a second kingdom claims the same leader, Bevy drops the
  older `LedBy`.
- **Naming:** `LedBy` mirrors Bevy's `LikedBy`; `Leads` is the read-only
  reverse. The manual `KingdomLedBy` reverse component is gone.

## Kingdom↔lands link is Bevy-native (`HeldBy`/`Holds`)

The kingdom→holdings link is a Bevy `#[relationship]` component `HeldBy`
(on each **land**, single `Entity`, source of truth) paired with the
auto-maintained `#[relationship_target]` `Holds` (`Vec<Entity>`) on the
kingdom. A land declares its kingdom; Bevy's hook keeps the kingdom's `Holds`
in sync — no manual `Vec`, no drift.

- **Direction flipped from the old model:** the data has kingdoms listing
  `land_ids`, but the relationship's single-`Entity` side lives on the land, so
  `populate` inserts `HeldBy(kingdom)` per land rather than a `Holds` Vec on the
  kingdom.
- **Naming:** `HeldBy` (land) / `Holds` (kingdom), the same active/passive
  mirror as `LedBy`/`Leads`. Reads go through `RelationshipTarget::iter`
  (in `bevy::prelude`), which yields owned `Entity`.

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
`BuildingOf(def_id)` to reach its stats and an `OnLand` relationship to its land.

- **Why entities, not the old `Built(Vec<String>)`:** the task asked for
  buildings to be addressable ECS entities with a relationship to the land, so
  construction/destruction, per-instance state (health, level, …), and scripting
  by instance id all have somewhere to live. Keeping `Built` alongside would be
  duplicated state — the architecture's single-source-of-truth rule rules that
  out, so `Built` is removed and the land's `BuildingsOn` target (the
  auto-maintained reverse of `OnLand`) replaces it.
- **Definition roster stays a resource.** Stats (profit, upkeep, levy,
  `construction_price`) are shared and read-only across every instance of a
  kind; copying them onto each entity would duplicate definition data and break
  the definition/state split. So a building entity holds the *def id* and looks
  the stats up, exactly as `Built`'s ids once did — just from the entity side.
- **Direction mirrors `HeldBy`/`Holds`:** the building is the child declaring its
  land (`OnLand`, single `Entity`, source of truth); the land's `BuildingsOn`
  auto-fills. Same active/passive mirror as the land↔kingdom link.
- **Instances are state-only,** like `Kingdom`: a `buildings:` section in
  `*.state.ron` is an id-keyed overlay (`merge_state` id-replaces, since a save
  holds the full set of what's built), and `reconcile` drops any instance whose
  `def_id` or `land_id` no longer resolves — the same repair-not-refuse policy.
- **Spawn order** gains a buildings step after lands and before kingdoms, so
  `OnLand` resolves to an entity that already exists.

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

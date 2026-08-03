# Decisions

Standing decisions for this project. Check here before designing anything;
append a new section when a decision is made.

## Single ECS world (no nested `Ctx.world`)

The simulation entities live directly in Bevy's App world. `Ctx` holds only
session state (rng, chronicles, `player_character_id`, `selected_region`);
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
  kingdom scans (replaced by the reverse `KingdomLedBy` component for O(1)
  character→kingdom lookup).

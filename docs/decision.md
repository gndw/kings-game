# Decisions

Standing decisions for this project. Check here before designing anything;
append a new section when a decision is made.

## Single ECS world, no nested `Ctx.world`

The sim entities live in Bevy's App world. `Ctx` holds only session state
(rng, player id, selection); `Chronicles` is its own resource; `Game` wraps
`Ctx` as a `Resource`.

- **Why:** Bevy 0.19's `World::query()` needs `&mut World`, so `Query` is
  friction-free only as a system param. A nested `Ctx.world` forced the
  hand-rolled `EntityIndex` + `&self`-reader machinery that `World::query`'s
  `&mut` requirement existed to avoid. Merging lets reads use `Query`
  directly and lets UI systems take system-param `Query`/`Res` inline.
- **Sim logic** (`recompute`, `payout`, `step`, selection stepping) lives
  as `&mut World` free functions run from *exclusive* systems because it
  mixes component mutation with resource reads — the one case `&mut
  World` (phased access) handles cleanly.
- **`Registry` stays** — the script ABI is string ids, so `id → Entity`
  lookup is needed; `Registry` is a resource on the App world.
- **Deleted:** `EntityIndex`, the read-model snapshot structs the UI used
  to need, and the O(n) kingdom scans (replaced by the auto-maintained
  leader collection).

## Character↔kingdom leader link is Bevy-native

The kingdom→leader link is a Bevy `#[relationship]` (on the kingdom, single
`Entity`) paired with an auto-maintained `#[relationship_target]`
(collection on the leader). Inserting the relationship on a kingdom has
Bevy's hook keep the leader's collection in sync — no manual reverse
insert, no drift.

- **Naming convention:** `<Attached-to><Verb-or-preposition><Target>` so
  the name tells you which entity the component sits on. The leader's
  collection has a public accessor (`character_leads.kingdoms()`); the
  underlying field is private (Bevy's correctness check requires it).
- **Multi-kingdom:** the leader's collection is `Vec<Entity>` — a
  character can rule several kingdoms (conquest transfer). Every call
  site that wanted "the actor's kingdom" had to become "any of the
  actor's kingdoms". `RelationshipTarget::iter` gives the walk for free.

## Kingdom↔land link is Bevy-native

Same shape as the leader link: a `#[relationship]` on the kingdom (single
`Entity`) auto-maintains a `#[relationship_target]` on the held land. The
target field is private with a public accessor (`land_held_by.kingdom()`).

## ECS components split to one field each

Markers (`House`/`Character`/`Land`) are bare tags; data is one field per
component. A system queries only the field it touches (payout needs gold
+ yield, not age), and Bevy tracks each mutable value independently.

## Definition + state: one struct per kind

`state`/`content` are merged into one struct per kind. Mods load in two
passes: definitions merge first (`merge`, id-replace), then state overlays
(`merge_state`, field-by-field). Two-pass so state can only fill entries
the definitions established.

- **Overlay never clobbers definition data.** `merge_state` copies only
  the state fields onto the matching content entry, so a state entry may
  carry only its state fields. Because the two field sets are disjoint,
  a single non-`Option` struct suffices — no `Option` overlay gymnastics,
  no parallel `State` map.
- **`State` (the parallel map) is gone.** `Content` carries kingdoms
  too. `reconcile(&mut Content)` repairs refs in place. `populate` reads
  the one struct.
- **Dropped:** the old "dropped state for unknown …" notes. With state
  folded into content there is no separate state map to diff against.

## Buildings are ECS entities; definitions stay a roster

Each built building is its own entity, not a string id on the land. The
*definition* per kind stays a resource roster (read-only, shared across
instances); an instance entity carries `BuildingOf(def_id)` to reach its
stats and a `BuildingOnLand` relationship to its land.

- **Why entities:** the task asked for buildings to be addressable ECS
  entities with a relationship to the land, so construction/destruction,
  per-instance state, and per-instance scripting have somewhere to live.
  The single-source-of-truth rule rules out a parallel `Built` list.
- **Stats stay on the def.** Copying profit/upkeep/levy onto each entity
  duplicates definition data and breaks the definition/state split.

## Player commands are self-describing

Each player command is a struct implementing a `Command` trait that owns
its rules (validation), its UI (a fixed run of selection steps reading the
world), and its effect (`execute`). The palette drives any registered
command's steps the same way; the roster is a `CommandRegistry` resource.

- **Why a trait + registry:** the earlier enum-dispatch model hardwired
  the palette to one command's flow. A self-describing trait lets each
  command define its own steps and the palette stays generic — the menu
  code is unchanged by new commands.
- **`step_items` takes `&World`, not `&mut World`** — it's a read. The
  helpers walk relationship targets via `world::get` rather than
  `world::query` (which needs `&mut World`).
- **`Arc<dyn Command>`** in the registry so the palette can move a
  command to `execute` (which needs `&mut World`) without holding the
  registry's borrow.
- **List computed in the exclusive `input`, rendered by the non-exclusive
  `update`:** `step_items` needs `&World`, which only an exclusive system
  gets. `input` recomputes the current list into the resource; `update`
  reads it.

## Camera is a boolean with a tween

Two views (whole map vs zoomed-on-selection) toggled by `Z`. Plain `bool`
on `Game`, not a state machine. `update_camera` reads the flag and the
current selection every PostUpdate frame and rewrites the camera's
projection + transform in place.

- **One frame, one write, every frame.** Recomputing a polygon's bbox is
  trivial; diffing `(mode, selection)` would be premature optimisation.
  Selection-following comes for free.
- **Fit via `AutoMin`**, not by hand — same projection the default view
  uses. Keeps the aspect-ratio guarantees and the 30%-zoom-in so the
  transition doesn't pop.
- **Smoothstep tween** between destinations. Two extra components on the
  camera entity (`CameraView` = current rendered view, `CameraTween {
  from, to, t }`). Each frame: (1) compute destination; (2) if it moved,
  restart the tween from the current rendered view; (3) advance `t` with
  a smoothstep ease; (4) write the lerped projection + transform. A tween
  settles exactly to the destination (`t = 1` ⇒ `view = to`), which keeps
  on-screen state clean for any later code that reads it. Exponential
  smoothing approaches asymptotically and would need a snap threshold.

## Building status is a serialized enum

`BuildingStatus` is both the ECS component and the RON state type with
`Active` / `Inactive` / `Building` variants. Variant names replace
numeric codes so invalid values cannot enter the world.

## Courtiers are state-only appointment entities

A courtier is an addressable state entity linking one character and one
kingdom through Bevy relationships (`CourtierOfCharacter` /
`CourtierOfKingdom` and their auto-maintained reverse collections).
`CourtierType` is an enum serialized by variant name; `Courtier` is the
generic role, leaving later roles additive without changing the entity
shape. The court panel follows the selected land's kingdom.

## A kingdom holds exactly one land

`Kingdom::land_id: String`, not `Vec<String>`. One kingdom rules
exactly one land; the game models war as the way to gain a new land.

- **Why data first:** the gameplay is 1:1 — a kingdom can't act across
  its lands any differently. The Vec carried no gameplay the 1:1
  doesn't, and the multi-land reconcile was paying complexity for an
  empty abstraction.
- **Why flip the Bevy shape too:** once the data side is 1:1, the
  runtime Vec is paying the same tax (`RelationshipTarget::iter` over
  one entity, callers guarding `.get(...).ok()` on a Vec of 1). The
  target is private with a public accessor — same pattern as the
  leader collection.

## War is an entity; the casus belli and the demands sit on it

A war is its own entity (not a marker on the kingdom). The casus belli
and the demands the war is fought over sit on the war as plain
components (`WarCasusBelliType`, `WarDemands`). There is no separate
`CasusBelli` entity kind.

- **Why a war entity:** the war sits between two kingdoms and a list of
  demands, all dynamic. An entity is the natural shape for a link in a
  graph, and the relationship-colocation rule keeps the source on the
  war and the reverses on the kingdoms.
- **Why no CB entity, just `WarCasusBelliType`:** the earlier `CasusBelli`
  entity was over-engineered for the gameplay we have. A war is the only
  consumer of a CB; CBs don't outlive the war that uses them in any
  current path, and "hoard a CB, press later" isn't on the roadmap.
  Folding the CB onto the war drops an entity kind, a relationship, and
  a reverse target. If hoarding lands later, splitting CB back out is
  mechanical.
- **Why `WarDemands` is a `Vec<WarDemand>`:** a war can carry multiple
  demands (a future `Conquest + Reparations` CB could seed two). The
  list lives on the war; `EnforceDemands` picks one to resolve at a time.
- **No automatic resolution.** The war has no status / no tick / no
  end condition — `EnforceDemands` is the explicit resolution step.
- **CB enum is the only place new CB shapes land.** `resolve_cb` is the
  one switchboard between a CB id and `WarCasusBelliType`; `demands_for`
  is the one place the CB shape's initial demand list lives. Adding a
  CB shape is a variant + a `resolve_cb` arm + a `demands_for` arm + a
  menu row.

## Conquest transfer is "add the player as the kingdom's leader" (multi-kingdom)

The `EnforceDemands` command's `Take` demand is a single
`KingdomLedBy(player)` insert on the target kingdom. Bevy's hook adds
the entry to the player's `Vec<Entity>` collection (the multi-kingdom
model) — the player keeps every kingdom they already led and gains the
conquered one. The defender's previous leader has the entry pruned.

- **Cost:** every site that read `character_leads.kingdom()` had to
  become "walk all kingdoms" (ruled lands, armies under, player wars),
  "any match" (rule checks, own-lands sets, kingdom predicates), or
  "pick one" (`Ctx::startup`, the DeclareWar attacker's pick). All
  mechanical.
- **No conquest cleanup on the defender's side (yet):** the defender
  loses `CharacterLeads` (Bevy prunes it), the defender's court
  appointments are released (the courtier entities despawn on `Take`).
  The defender's `LandHeldBy`, treasury, and other state stay intact
  until the conquest transfer code lands.

## Relationship components live in the file of their main component

Every relationship component — both sources and reverses — is placed in
the file of the entity it sits on. The relationship's behaviour is
determined by which entity it attaches to, so its file should match.

- **Rule for new relationships:** when adding a `#[relationship]` or
  `#[relationship_target]`, decide the entity it attaches to first,
  then put the component in that entity's file. If a future generic
  relationships module appears, move them back — the split by entity
  kind is the default.

## `BuildingDef::levy_rate` is the per-month replenishment of an army

Military building definitions carry `levy_rate: u32` (defaulted to 0 on
civil kinds). It's the per-month levy contribution a building makes to
armies raised on its land.

- **Why a separate field:** the static `levy` is the immediate
  contribution to the realm's standing pool, summed at
  construct/destroy. `levy_rate` is the *flow* into the army's levy
  over time. The two are independent.
- **Rule of thumb:** `levy_rate = max(1, round(levy * 0.025))`. A mod
  can override per kind.

## Army and Marching are separate entity kinds

A marching is a separate entity from the army it moves, not a component
on the army. The marching carries the scheduling data (the two
endpoints, the road, the dates, the status); the army carries the
operational state (`ArmyStatus` + `ArmyMarching` pointing at the
current marching). They are linked by a Bevy relationship —
`MarchingArmy` (on the marching) ↔ `ArmyHasMarching` (a `Vec<Entity>`
on the army, the queue, insertion-order-preserving).

- **Why a separate entity:** the same army can carry multiple marchings
  at once (a queue of marches; even a single ordered move is usually a
  chain because one marching covers one road). Putting the data on the
  army would have turned `Army` into a `Vec` of marchings. The marching
  entity keeps the data shape small and lets the daily tick walk a
  single archetype.
- **Why a relationship, not a plain `Entity` field:** Bevy's
  hook-maintained `Vec` collection on the army is the queue, free of
  hand-maintained reverse inserts. Insertion order is preserved by the
  `RelationshipTarget` Vec, which is how "first scheduled marching on
  the matching source land wins" lands.
- **Current vs scheduled marching.** `ArmyMarching` (single `Entity`
  on the army) is set only by the daily tick when it activates a
  scheduled marching, and removed when the queue runs dry. The Vec
  includes both the current marching and the scheduled ones waiting.
- **Run-time only.** Marchings never appear in mod data — spawned by
  the marching command, despawned by the daily tick as each road is
  finished. `DismissArmy` walks the queue and despawns every marching
  before despawning the army.

## One marching per road; armies travel the road network

A marching entity covers exactly one road. `MarchingOnRoad` names it
and `MarchingFromLand` / `MarchingToLand` are always that road's two
ends. An order to a land further off is not one long marching — the
marching command traces the road graph and spawns one marching per
road on the route, queued in travel order.

- **Why per-road:** the road is where a marching *happens*. Anything
  reasoning about a moving army (where it is between two lands, who
  else is on that road, an ambush, a blocked road) needs the road, and
  a marching that spanned several roads would have no single answer.
- **Route tracing, not free movement.** Armies only move along roads.
  The command breadth-firsts the graph (built by walking every
  `Road`'s ends), so the **fewest-roads** route wins — not the fewest
  days; with per-road costs those can differ, and hop count is the
  cheaper, more predictable rule until the map is big enough for the
  difference to bite. A target with no chain of roads is rejected
  outright.
- **Why `MarchingOnRoad` is a relationship** while the road's own
  `RoadBetweenLands` is a plain `Vec<Entity>`: the road→land link is
  definition data baked at populate, but marchings are spawned and
  despawned constantly — exactly the churn Bevy's hook-maintained
  reverse handles.

## March duration is per-road, authored data

How long a march takes is a property of the road, not a constant. Each
road in mod data carries `distance_days`, loaded into the
`RoadDistanceDays` component and read by `game::marching::road_days`.
The daily tick sets a marching's arrived date to `begin + road_days(its
road)`; the marching command sums the same values across a traced
route to quote the player a total.

- **Authored, not computed from `points`.** Length is only a proxy for
  effort. Deriving the days at load would make the number a function of
  how the polyline happens to be drawn — nudge a holding and every
  march in the region silently re-prices. As data, a mod can make a
  paved highway cheap or a mountain pass dear without redrawing.
- **Zero is fatal.** `validate` rejects `distance_days: 0`: an army
  would begin and arrive the same day and (with the tick's `today
  >= arrived` test) teleport the whole route.
- **One resolver, no fallback.** `road_days` is the only place the
  component is read, so the number the command quotes and the number
  the tick charges cannot drift. It returns `Option<u32>` and there is
  no default duration to fall back on: a road without one is a torn
  world, and inventing a number there would hide the bug behind armies
  that march a plausible-looking length of time. Callers refuse to
  move instead. The one case needing care is a `false` mid-route,
  after the finished marching has been despawned: the army must
  `stand_down` there, because leaving it `Marching` with an
  `ArmyMarching` pointing at a despawned entity would freeze it
  permanently.

## Siege is its own entity kind; conquest pauses the realm economy

A siege is a separate entity, not a marker on the army or the land. It
carries the scheduling data (`SiegeProgress`, `SiegeNextEventDate`) plus
two relationships to the belligerents. The army carries its operational
state (`ArmyStatus::Sieging`); the land is the target.

- **Why an entity, not a component on the army:** an army can stand on
  a foreign land without laying siege to it — the siege is a decision
  the player takes, not a state the army is always in. Putting it on
  the army would have meant every foreign-land army carries siege
  fields it doesn't need.
- **Why conquest flips every building on the land to `Inactive`:** the
  visible consequence of losing a land is the economy stopping. Until
  the conquest-transfer code lands, this is the player's only signal
  that something happened — the realm keeps its `LandHeldBy` until war
  resolution moves it, but its buildings stop contributing until then.
- **`ArmyHasSiege` is single, `LandHasSiegesUnderAttack` is `Vec`.**
  One army = at most one siege; a land can be under attack from
  multiple armies at once.
- **`SiegeProgress` is per-siege, not per-attacker.** All armies
  besieging the same land march at their own pace; the per-siege
  counter is the unit of resolution.
- **No resolution of `LandHeldBy` (yet).** When a siege wins, the
  conquering army gets `ArmyControlsLand` but the defending kingdom's
  `LandHeldBy` stays put. The war-resolution piece is the obvious next
  step.

## Command palette search is a palette-owned overlay

The command palette has a search bar above the list; typed characters
move matches to the top and dim the rest in place. The search is owned
entirely by `ui::command_menu.rs` — each command still returns the same
flat `Vec<MenuItem>` from `step_items`, and the palette reorders, dims,
and clamps around the result. Commands don't know about the search.

- **Why palette-owned:** every command's `step_items` returns a
  `Vec<MenuItem>` with a `label` and `value`. The palette is the one
  place where all of them meet, so it can apply one filter once and
  reach every list. Threading a search context into every command's
  `step_items` would have meant a new parameter on the trait (and a
  new branch in every arm) for the same one substring test.
- **Substring match on the label, case-insensitive.** No fuzzy match,
  no prefix weighting, no per-item scoring. With the base game's eight
  commands and the few dozen items each step returns, a fancier ranker
  would be invisible. If a mod grows a roster to where ordering by
  relevance matters, swap the matcher in `refresh` — the rest of the
  system (the `matches` bit vec, the reorder, the cursor snap) is
  agnostic to *how* an item matched.
- **Cursor navigates matches only when the query is non-empty.** With
  an empty query every item matches; with a query the cursor wraps
  around the matches and ignores the dimmed rows beneath. The dimmed
  rows are still selectable.
- **Query clears on every panel change** (top-level ↔ step) and on
  open/close. Each panel gets a fresh filter.
- **`Space` is yielded to the palette while it's open.** Multi-word
  queries are the obvious use case; without yielding the keystroke,
  every space would also toggle `Game::paused`.
- **Why the keyboard reading uses a stored `MessageCursor`, not
  `MessageReader`.** The palette's `input` is exclusive (it needs
  `&World` for `step_items` and `&mut World` for `execute`), so it
  can't take a `MessageReader` system param. A `MessageCursor<...>` on
  the `CommandMenu` resource is cloned out before the exclusive borrow
  on `Messages<KeyboardInput>`, then written back.

## Chronicle generation lives in its own observer module

The chronicle text is split out from game-logic code into one module
that owns one observer per chronicle-worthy event. Commands and ticks
only `world.trigger(...)`; the observers read display names off the
world and write one past-tense line per event.

- **Why:** keeps game-logic code free of string formatting and
  `Chronicles` access. Chronicle text lives in exactly one place, so
  a future "rewrite the voice" pass is a one-file change. The flavor
  (past tense, third person, lands named, ids hidden) is enforced by
  the module boundary — nothing else can push a line.
- **Event surface.** The module observes everything that can produce
  a chronicle line: `OnBuildingUpdated` (with a `kind` dispatch),
  `OnArmyRaised` / `OnArmyDismiss`, `OnMarchingOrdered` /
  `OnArmyArrived` (with `continuing: bool`), `OnSiegeLaid` /
  `OnSiegeWon`, `OnWarDeclared` (with the casus belli), `OnDemandEnforced`
  (with the demand type), and `OnWarEnded`. New events are additive.
- **Mechanic words are banned.** "active", "conquest", "Take enforced",
  "in field" — these are code words. The chronicle reads "now in
  operation", "demanding its lands", "taking the crown", "home".
- **Subject for player-driven events.** The observer module resolves
  the player character once per observer batch via a `PlayerCtx`
  `SystemParam` and formats the actor as "You".

## Army formation is a per-day accrual

Raising an army is no longer instantaneous. The army starts in
`ArmyStatus::Raising` with `ArmyLevy = 0` and `ArmyMaxLevy = sum of
available BuildingLevy pools on the raise land at raise time`; the
per-day formation tick accretes up to 20 levy per ACTIVE
`BuildingIsRaised` building on the army's land per day into `ArmyLevy`,
then flips the army to `Idle` once `ArmyLevy >= ArmyMaxLevy`. Buildings'
pools are not drained at raise time — the formation tick drains them
incrementally.

- **Why a separate `Raising` state.** The user-visible behavior is
  "the army is forming" — the player needs a state to tell them the
  army exists but isn't ready yet, and the marching / siege / dismiss
  commands need to know not to treat it as a normal army. Folding it
  into `Idle` would have meant every reader branching on `ArmyLevy <
  ArmyMaxLevy`, which is exactly what a state is for; folding it into
  `Marching` would have stolen `ArmyMarching`'s slot.
- **Why `ArmyMaxLevy` is its own component, not derived.** `ArmyLevy <
  ArmyMaxLevy` could be computed from a single `ArmyLevy(u64, u64)`,
  but the player-facing read is "0/120 → 20/120 → 60/120 → 120/120",
  which the per-day tick updates one field of. Splitting them keeps
  each `get_mut` to the single field that actually changed.
- **Why a per-day 20 cap per building.** It's the smallest cap that
  makes formation timing observable to the player — a single
  `levy: 30` barracks needs two days to fill, a `levy: 200` across
  ten buildings needs one. Constant caps would lose the "more
  buildings = faster muster" signal.
- **Why not drain the pools at raise time.** The pool value is the
  realm's static levy budget; spending it at the moment of raise
  would have made the per-day formation invisible. Draining as the
  army forms keeps the building rows in the `buildings` panel showing
  `30/30 → 10/30 → 0/30` over the formation days. The flag
  (`BuildingIsRaised = true`) prevents the monthly `replenish_levy`
  from competing with the formation drain.
- **Building selection is "ACTIVE + `BuildingIsRaised = true`", not
  "every ACTIVE building on the land".** A building constructed
  mid-formation isn't flagged by the original raise, so it doesn't
  contribute to the muster. The asymmetry is fine: raising is a
  one-shot moment the formation can record (flag the buildings);
  dismissing is a final accounting (pour into the whole land).
- **Marching is gated to `Idle`.** The marching tick matches on
  `ArmyStatus::Idle` to find scheduled marchings to flip `OnRoute`;
  a `Raising` army whose land already has a scheduled marching sits
  in the queue untouched.
- **Chronicle text reads "raising" while `ArmyLevy == 0`.** The
  `on_army_raised` observer branches on the army's status: a raising
  army says "You began raising the Lannister Army at X — up to N
  spears gathering for the muster.", a filled army says the original
  "N spears answering the call." line.

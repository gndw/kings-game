# Decisions

Standing decisions for this project. Check here before designing anything;
append a new section when a decision is made.

## Decision file holds code patterns, not game logic

`docs/decision.md` documents the *patterns* the codebase uses — the shape
of relationships, the load sequence, the trait registry, the observer
split, the camera tween — not the feature-by-feature design of every
game entity. A pattern is reusable across the codebase; a game-logic
decision is specific to one entity kind (a `War`, a `Building`, a
`Marching`) and grows with content.

- **Why:** every new CB shape, building kind, army type, or siege
  mechanic would otherwise add a section. The file grows with content
  instead of with patterns, and reviewers have to ask "is this a code
  pattern or a feature?" every time something lands.
- **Test:** if the decision applies to one entity kind only, it
  doesn't belong here. If it applies across kinds (every relationship
  follows `<On-entity><Verb><Target>` naming, every command follows the
  trait pattern, every chronicle event has one observer), it does.
- **Where game-logic decisions live:** the rustdoc on the entity kind
  itself, or the `ecs/<kind>.rs` module doc. Self-contained game rules
  stay with the code they describe; decision.md is the cross-cutting
  pattern file.

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

## Relationship components live in the file of their main component

Every relationship component — both sources and reverses — is placed in
the file of the entity it sits on. The relationship's behaviour is
determined by which entity it attaches to, so its file should match.

- **Rule for new relationships:** when adding a `#[relationship]` or
  `#[relationship_target]`, decide the entity it attaches to first,
  then put the component in that entity's file. If a future generic
  relationships module appears, move them back — the split by entity
  kind is the default.

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

## Game system files use verb-ing (gerund) names

Every file under `src/game/` is named with the present participle of the
action its system performs: `aging`, `advancing_date`, `besieging`,
`building_releasing`, `constructing`, `court_releasing`, `marching`,
`paying_out`, `raising_army`, `replenishing_levy`, `yielding`. The name
describes what's *currently happening* in the system (a date is being
advanced, levies are being replenished), not what kind of object the
code touches.

- **Why:** the modules under `src/game/` are *scheduled ticks* — they
  fire on `OnDay` / `OnMonth` and re-run forever while the game is
  alive. They're not entities or one-shot operations. A noun (`payout`,
  `siege`, `yields`) reads as "this is a thing"; a gerund reads as
  "this is ongoing". The module list at the top of `src/game.rs` is
  then a list of verbs, which matches the schedule it owns.
- **Phrasal verbs:** keep the preposition as an underscore.
  `paying_out` (from "pay out"), not `payouting`. Ugly English, but
  the only honest gerund.
- **Archaisms:** "siege" as a verb is archaic — use `besieging`.
- **Function names follow how the system is invoked.**
  - Schedule-driven systems name their entry point after the schedule
    that runs them: `advancing_date::tick` (`FixedUpdate`),
    `constructing::on_day`, `marching::on_day`, `besieging::on_day`,
    `paying_out::on_month`, `replenishing_levy::on_month`. The function
    name tells you where it fires without reading the registration.
  - Observer-driven systems keep the `on_<event>` name
    (`yielding::on_building_updated`, `building_releasing::*`,
    `court_releasing::*`).
  - Pure helpers keep a name that describes what the call computes —
    `aging::age` derives an age, `marching::road_days` returns a
    duration. They have no schedule; naming them after one would lie.
- **Scope is `src/game/` only.** Other layers hold nouns by design
  (entities, commands, resources). The gerund rule is a fit for
  scheduled-tick modules; forcing it elsewhere would mis-name code
  that isn't a running system.

## Wiki navigation is a left-hand tree

The wiki uses a left navigation tree and a right details panel. Arrow up/down
moves only through visible nodes; arrow right expands the selected node and
arrow left collapses it. Selection owns the details shown in `WikiBody`, so
adding another wiki item means adding its tree node and renderer, not a new
navigation state system. `Houses` is the only root item today.

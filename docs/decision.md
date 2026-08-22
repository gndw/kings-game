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
the definitions established — except for alive characters, whose full
record lives in state (see below).

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
- **`validate` runs after both passes.** Originally it ran between
  passes (so state could only fill known slots). Alive characters now
  live entirely in state, so the entry must exist by the time we check
  house_id / skills. `reconcile` still runs last, dropping dangling
  state refs for the future save-loading path.

## Characters split by alive/dead, not by def/state

The old split — identity in `characters.ron`, mutable overlay in
`start.state.ron` — left every alive character with parts in both files,
which meant a save couldn't reconstruct one without the constants file.
Now:

- **`characters.ron` holds dead characters only.** Their data never
  changes in play, so it belongs in the read-only constants file
  (which a save file will never include).
- **`start.state.ron` holds alive characters only**, with the full
  record (name, house, gender, skills, dob, gold, next_death_event_date).
  The sim mutates these fields; the save will write them.

`merge_state` now inserts a character outright if no constants entry
exists for that id (the alive case); otherwise it overlays state fields
as before. Mods that want to introduce a new alive character write the
full record into their own `start.state.ron`; mods that want to tweak an
existing base character overlay the mutable fields they care about.

The same split extends to `Family` entries: a family that references
any alive character moves into `start.state.ron` (the alive char is
state-only — putting the family in `families.ron` would dangle before
state has loaded); a family that references only dead characters stays
in `families.ron` (no state dependency, fully constant). `merge_state`
treats `families` the same way it treats `buildings` — id-replace — since
family entries have no mutable fields.

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
    `age_helper::get_age` derives an age, `marching::road_days` returns a
    duration. They have no schedule; naming them after one would lie.
- **Scope is `src/game/` only.** Other layers hold nouns by design
  (entities, commands, resources). The gerund rule is a fit for
  scheduled-tick modules; forcing it elsewhere would mis-name code
  that isn't a running system.

## Events use `On<PastTense>` names

Every event in `src/events.rs` follows the same shape: `On<Entity><PastTense>`
(`OnBuildingUpdated`, `OnArmyRaised`, `OnMarchingOrdered`, `OnSiegeWon`,
`OnDemandEnforced`, `OnWarEnded`). Past tense — the event fires after the
thing happened, never before. The chronicle module reads this in the doc
("commands and ticks only `world.trigger(...)`") and one observer per event
writes one past-tense line. A new event lands as a `On<PastTense>` struct,
a trigger site, and an observer — three additions, no renames.

The event popup uses `OnEventPresented` and `OnEventResolved`; both follow
the same `On<PastTense>` shape. `OnEventResolved.choice: Option<usize>`
encodes both the picked-choice path (`Some(idx)`) and the forfeit path
(`None` for `Esc`) — the resolver interprets `None` as "no effect, just
clear pending and reschedule".

## Gold is a realm treasury, not a leader's purse

The kingdom owns its gold. `KingdomGold` (signed) is the realm treasury;
`KingdomGoldYield` is the realm's net monthly income; `KingdomLevy` is
the realm's available troops. `Character` carries none of these — the
leader is the steward of an existing treasury, not its owner.

- **Why:** keeping the gold with the realm means it survives leadership
  change. When a ruler dies, the realm's existing treasury passes to
  the heir unchanged — the new leader inherits a pre-existing war
  chest, not an empty pot. The old model (a "personal purse" on the
  character that was transferred to the heir on succession) was a
  bookkeeping fiction: it was the same number moved across an entity
  boundary, with no game effect. Folding the gold into the kingdom
  drops the transfer step and the loss-on-no-heir step in one go.
- **Each kingdom is independent.** A character ruling several kingdoms
  has access to each kingdom's own treasury. Commands route the gold
  move to the kingdom that owns the resource: `construct_building` pays
  from the land's kingdom; `gift_gold` debits the source's primary
  kingdom. The resource bar at the top of the screen sums the player's
  realms into one number (the "how rich am I" reading); the kingdom
  panel drills into one realm.
- **Personal gifts don't credit the target's kingdom.** When the player
  gifts gold to another character, the realm treasury is debited; the
  gold leaves the giver's books and is *not* re-booked to the
  recipient. A coin handed to a stranger in the hall doesn't go into
  the stranger's realm's treasury — it just leaves. The recipient
  still gains a `ReceivedGold` memory that boosts their opinion of the
  giver. The same rule applies to script-driven `transfer_gold`: debit
  the giver, no credit. This keeps the script API compatible (it's
  still "from this person to that person") while staying honest about
  the medieval-economic reading.
- **Replacing the model required moving the levy too.** The character
  previously held `levy` and `gold_yield` too — summed across all
  ruled kingdoms. After the move each kingdom has its own, computed
  from its own land, so a multi-kingdom leader no longer needs the
  sum-on-read in their own entity. The resource bar still sums for
  display; the per-kingdom values are what the sim writes.
- **Script view: `c.realm`.** The character view exposed to Rhai events
  drops the old `c.gold` and `c.levy` fields. They become `c.realm`
  — a map carrying the character's first ruled kingdom's `name`,
  `gold`, `gold_yield`, and `levy`, or `()` if the character doesn't
  rule one. Modders can use `c.realm != ()` as "is a ruler" or filter
  by `c.realm.levy > 0` for "has troops". The base mod's events use
  this — the foreign knight filters `c.realm != () && c.realm.levy > 0`,
  the wayfaring stranger filters the inverse.
- **Migration in RON.** The starting `gold: N` field moves from each
  alive ruler's character entry to the matching kingdom entry. Non-ruler
  characters' starting gold is dropped (they have no realm). `levy` and
  `gold_yield` were never authored in the RON — the sim has always
  recomputed them at startup — and remain unreferenced from the data
  file, now at the kingdom level.

# Modding Architecture Design

> Goal: almost every aspect of kings-game is data-driven and moddable.

## Directory layout

```
mods/
  base/
    mod.ron              # metadata: name, version, dependencies
    map.ron              # geometry: border + lands (polygons + holdings)
    buildings.ron        # building templates (gold/levy stats)
    houses.ron           # families
    characters.ron       # people
    kingdoms.ron         # realms (references land + character IDs)
    calendar.ron         # time: days per month/year, starting date
    rules.ron            # economy + military constants
    theme.ron            # UI: colors, font sizes, panel proportions
    events/              # event definitions
      economy.ron
      warfare.ron
      intrigue.ron
```

The `KINGS_MODS` env var points at one or more mod directories,
colon-separated, loaded left-to-right. Later mods override same IDs,
add new ones.

```
KINGS_MODS=mods/base                          # just the base game
KINGS_MODS=mods/base:mods/my_expansion        # stack an expansion
```

Backward compat: if `KINGS_MODS` is unset, fall back to the current
`KINGS_MAP` single-file path — or the default `mods/base`.

---

## 1. Splitting map.ron into definition files

### map.ron (geometry only)

```ron
(
    border: (x0: 0, y0: 40, x1: 860, y1: 540),
    lands: [
        (
            id: "land-1",
            name: "westwatch",
            holding: (110, 115),
            borders: [(180, 155), (130, 145), (80, 140), /* ... */],
        ),
        // ...
    ],
)
```

No `building_ids` here — those move to `kingdoms.ron` (a kingdom
decides what's built in its lands) or stay on the land if you prefer
"buildings belong to geography." Design call; I'd put them on
kingdoms since that's who builds and pays.

### buildings.ron

```ron
(
    buildings: [
        (id: "building-barracks", name: "barracks",
         gold_upkeep: 5, levy: 50),
        (id: "building-market", name: "market square",
         gold_profit: 10),
        // ...
    ],
)
```

Unchanged from today, just its own file.

### houses.ron

```ron
(
    houses: [
        (id: "house-hightower", name: "hightower"),
        (id: "house-lannister", name: "lannister"),
        (id: "house-arryn", name: "arryn"),
    ],
)
```

### characters.ron

```ron
(
    characters: [
        (id: "char-leyton", name: "leyton",
         house_id: "house-hightower", age: 61),
        // ...
    ],
)
```

### kingdoms.ron

```ron
(
    kingdoms: [
        (
            id: "kingdom-west",
            leader_character_id: "char-leyton",
            seat_land_id: "land-2",
            land_ids: ["land-1", "land-2"],
            // buildings now belong to the kingdom-land pair
            buildings: {
                "land-1": ["building-tannery", "building-wharf",
                           "building-vineyard", "building-watchtower"],
                "land-2": ["building-mill", "building-smithy",
                           "building-vineyard", "building-archery-range"],
            },
        ),
        // ...
    ],
)
```

### Why split buildings onto kingdoms?

Two reasons:
1. **Who pays?** Kingdoms hold the treasury. If buildings belong to
   geography, you still need to know which kingdom owns them for
   economy. Putting them on the kingdom keeps the chain clean.
2. **Conquest.** When kingdom A takes land-3 from kingdom B, the
   buildings move with the land — and the new owner gets the
   gold/levy. Storing them as `kingdom → land → [buildings]` makes
   transfer a move between keys, not a re-tag.

If you prefer buildings-on-lands, that's fine too — the merge logic
is the same either way.

---

## 2. Rules config

Everything currently a hardcoded constant becomes data.

### calendar.ron

```ron
(
    days_per_month: 30,
    months_per_year: 12,    # implicit from days_per_year / days_per_month,
                            #   but explicit is clearer for modders
    days_per_year: 360,
    start_date: (year: 1066, month: 1, day: 1),
    month_names: [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ],
    season_months: {
        spring: [3, 4, 5],
        summer: [6, 7, 8],
        autumn: [9, 10, 11],
        winter: [12, 1, 2],
    },
)
```

### rules.ron

```ron
(
    economy: (
        starting_gold: 500,
        tax_per_building: 1.0,       # multiplier on building gold_profit
        upkeep_multiplier: 1.0,      # multiplier on building gold_upkeep
        trade_route_bonus: 0.15,     # per adjacent friendly land
        bankruptcy_threshold: 0,     # gold can't go below this
    ),
    military: (
        base_levy_efficiency: 0.8,   # fraction of levy that fights
        reinforcement_rate: 5,       # troops per day
        attrition_winter: 0.05,      # fraction of levy lost per winter month
        siege_days_per_holder: 30,   # base time to siege a holding
    ),
    character: (
        adult_age: 16,
        max_age_variance: 20,        # random lifespan baseline
        marriage_age_min: 16,
        succession_type: "primogeniture",  # later: "election", "partition"
    ),
)
```

These become structs loaded at startup, not `const` values. The `Ctx`
holds a reference to them; the simulation reads from them.

---

## 3. Theme config

```ron
(
    font_size: 14.0,
    panel_gap: 6.0,
    panel_bg: (r: 0.0, g: 0.0, b: 0.0, a: 0.6),
    title_color: (r: 0.75, g: 0.7, b: 0.45),
    chronicle_height_pct: 30.0,
    right_bar_pct: 30.0,
    map_colors: (
        border: "blue",
        land_outline: "white",
        holding: (r: 0.59, g: 0.29, b: 0.0),
        selected_outline: "yellow",
        selected_holding: "yellow",
    ),
    flag: (
        cloth_color: "red",
        pole_height: 28.0,
        cloth_width: 18.0,
        cloth_height: 10.0,
    ),
)
```

Colors can be either named CSS (`"blue"`, `"red"`) or RGB tuples.
The loader resolves them to `bevy::color::Color`.

---

## 4. Event system (declarative — Path A)

### Design

An event has:
- A unique ID
- A **trigger** (conditions that must be true)
- A **weight** (how likely, relative to other eligible events)
- A set of **effects** (what happens)
- Optional **cooldown** (min days before it can fire again)
- Optional **scope** (which entity it applies to: kingdom, land, character)

```ron
(
    events: [
        // --- Economy events ---
        (
            id: "event-good-harvest",
            scope: "kingdom",
            weight: 5,
            cooldown_days: 180,
            conditions: [
                SeasonIn(["spring", "summer"]),
                KingdomGoldAbove(50),
            ],
            effects: [
                AddGold(30),
                Chronicle("{ruler} of {kingdom} rejoices — a bountiful harvest fills the granaries."),
            ],
        ),
        (
            id: "event-famine",
            scope: "kingdom",
            weight: 10,
            cooldown_days: 360,
            conditions: [
                SeasonIn(["winter"]),
                KingdomGoldBelow(100),
            ],
            effects: [
                AddGold(-50),
                AddLevyPercent(-0.1),
                Chronicle("Famine stalks {kingdom}. {ruler}'s subjects cry out for bread."),
            ],
        ),
        // --- Character events ---
        (
            id: "event-character-death-old-age",
            scope: "character",
            weight: 100,
            conditions: [
                CharacterAgeAbove(65),
                RandomChance(0.02),   # per check, ~2% per day once eligible
            ],
            effects: [
                KillCharacter,
                Chronicle("{character} of {house} has died at age {age}."),
                TriggerEvent("event-succession"),
            ],
        ),
        // --- Warfare events ---
        (
            id: "event-border-skirmish",
            scope: "land",
            weight: 8,
            cooldown_days: 90,
            conditions: [
                HasNeighborKingdom,         # land borders a different kingdom
                NotAtWar,
            ],
            effects: [
                AddLevy(-10),
                AddRelation(from: "kingdom", to: "neighbor_kingdom", delta: -5),
                Chronicle("Border skirmish near {land} strains relations between {kingdom} and {neighbor_kingdom}."),
            ],
        ),
    ],
)
```

### Condition enum (Rust)

```rust
#[derive(Deserialize, Clone)]
#[serde(tag = "type", content = "value")]
enum Condition {
    SeasonIn(Vec<String>),
    KingdomGoldAbove(i64),
    KingdomGoldBelow(i64),
    KingdomLevyAbove(u64),
    KingdomLevyBelow(u64),
    CharacterAgeAbove(u32),
    HasNeighborKingdom,
    NotAtWar,
    AtWar,
    RandomChance(f64),
    RelationBelow { with: String, threshold: i32 },
    Custom(String),   // modder-defined, matched against registered checks
}
```

### Effect enum (Rust)

```rust
#[derive(Deserialize, Clone)]
#[serde(tag = "type", content = "value")]
enum Effect {
    AddGold(i64),
    AddLevy(i64),
    AddLevyPercent(f64),
    AddRelation { from: String, to: String, delta: i32 },
    KillCharacter,
    ReplaceRuler { character_id: String },
    TriggerEvent(String),
    SetFlag(String, Value),       // set a persistent world flag
    ClearFlag(String),
    Chronicle(String),            // template string with {placeholders}
    GiveBuilding { land_id: String, building_id: String },
    RemoveBuilding { land_id: String, building_id: String },
    ChangeControl { land_id: String, to_kingdom_id: String },
}
```

### The event loop (per tick)

```
1. For each scope entity (kingdom, land, character):
   2. Filter events matching this scope.
   3. Evaluate conditions (short-circuit on first false).
   4. Collect eligible events with their weights.
   5. Pick one via weighted random (if any).
   6. Check cooldown — skip if on cooldown.
   7. Apply effects.
   8. Register cooldown.
```

Not every scope fires every day — that would be noisy. A reasonable
default: check kingdom-scoped events every 30 days (monthly council),
character events daily (age/death), land events every 7 days.

This cadence goes in `rules.ron`:

```ron
event_cadence: (
    kingdom_days: 30,
    character_days: 1,
    land_days: 7,
)
```

### Template strings

`{kingdom}`, `{ruler}`, `{land}`, `{character}`, `{house}`, `{age}`
resolve against the current scope. A simple `format!`-like resolver
that replaces known tokens. No full template engine needed.

---

## 5. Mod loading + merge

### mod.ron

```ron
(
    name: "base",
    version: "0.1.0",
    game_version: "0.1.0",   # kings-game version this mod targets
    dependencies: [],         # other mod names that must load first
    replaces: [],             # IDs this mod wholly replaces (not merges)
)
```

### Loader pseudocode

```rust
struct ModLoader {
    mods: Vec<PathBuf>,  // resolved mod directories in load order
}

impl ModLoader {
    fn load(&self) -> Result<World> {
        let mut buildings = IndexMap::new();  // id → Building
        let mut lands = IndexMap::new();
        let mut kingdoms = IndexMap::new();
        // ... etc for each file type

        // Load each mod dir in order, merging
        for mod_dir in &self.mods {
            merge_file(&mut buildings, &mod_dir.join("buildings.ron"))?;
            merge_file(&mut lands, &mod_dir.join("map.ron"))?;
            merge_file(&mut kingdoms, &mod_dir.join("kingdoms.ron"))?;
            // ...
        }

        // Cross-validate all references
        validate(&buildings, &lands, &kingdoms, /* ... */)?;

        Ok(World { buildings, lands, kingdoms, /* ... */ })
    }
}

/// Insert-or-overwrite by ID. Later mods win.
fn merge_file<T: Identifiable>(
    map: &mut IndexMap<String, T>,
    path: &Path,
) -> Result<()> {
    if !path.exists() { return Ok(()); }  // optional file
    let parsed: Vec<T> = ron::from_str(&fs::read_to_string(path)?)?;
    for item in parsed {
        map.insert(item.id().to_string(), item);  // overwrite = replace
    }
    Ok(())
}
```

`IndexMap` preserves insertion order (so base game items appear
before mod-added items in the UI) and allows O(1) lookups.

### Merge semantics

| Situation | Result |
|---|---|
| New ID | Added |
| Same ID in later mod | Replaced (whole entry) |
| Same ID, partial override | Not supported — replace the whole entry |

Whole-entry replacement is simpler and avoids deep-merge ambiguity.
If a modder wants to tweak one building, they copy the base entry
and edit it. Paradox mods work this way.

### KINGS_MODS resolution

```rust
fn resolve_mod_dirs() -> Vec<PathBuf> {
    match env::var("KINGS_MODS") {
        Ok(v) => v.split(':').map(PathBuf::from).collect(),
        Err(_) => {
            // backward compat: single map file or default mod dir
            if env::var("KINGS_MAP").is_ok() {
                vec![]  // old single-file path
            } else {
                vec![PathBuf::from("mods/base")]
            }
        }
    }
}
```

---

## 6. What stays hardcoded

Things that are structural, not content:

- The condition/effect enum variants (adding new ones needs a code change)
- The Bevy system scheduling (when to draw, when to tick)
- The core loop: condition → weight → effect
- Camera/viewport math

A modder can combine existing conditions/effects in new ways, add
new data, reskin the UI, change all constants — but can't add
fundamentally new mechanics without a Rust change. This is the same
trade-off as Paradox's scripted triggers/effects (they have more
primitives, but still a fixed vocabulary).

If that ceiling becomes too low, Rhai scripting (Path B) plugs in
here: a `Scripted(String)` variant in both Condition and Effect that
delegates to a Rhai file. Everything else stays the same.

---

## 7. Implementation order

1. **Split `map.ron`** into `mods/base/*.ron` + add the `ModLoader`
2. **Add `calendar.ron`** — replace the constants in `ecs.rs`
3. **Add `rules.ron`** — add a `Rules` resource, wire into `Ctx`
4. **Add `theme.ron`** — replace the constants in `ui/mod.rs`
5. **Add `events/`** — Condition/Effect enums, the per-tick event
   checker, base event pack
6. **Cross-mod validation** — friendly error messages for broken refs

Steps 1–4 are mechanical refactors (move data, add structs, wire up).
Step 5 is the real design work and should come after the simulation
has at least basic economy (gold/levy per tick) so events have
something to act on.

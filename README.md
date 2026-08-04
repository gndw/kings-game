# kings-game

A small grand-strategy sandbox in Rust: a hand-drawn island of three realms,
a 360-day calendar, and a chronicle that writes itself one simulated day at a
time. Bevy draws it, [hecs](https://docs.rs/hecs) simulates it.

## Running

```sh
make play              # release build — the one you actually play
make run               # debug build with dynamic linking
make play SEED=1066    # a specific campaign
make play PLAYER=char-jon   # play as someone else
make check             # fmt + clippy + tests
```

The binary takes the seed as its first argument and requires
`--player-character-id <id>` — any character id the loaded mods declare. There
is no default player; `make` supplies `char-tywin` unless you pass `PLAYER`.

Needs a Rust toolchain (edition 2024) and X11 dev libraries. Bevy is built
without wayland, audio, or 3d.

## Controls

| Key | |
|---|---|
| `space` | pause / unpause |
| `+` / `-` | step through the speeds in `calendar.ron` |
| `q`, `esc` | quit |

## Modding

A mod is a folder under `mods/`. Folders load in sorted name order, so a later
one wins; point `KINGS_MODS` somewhere else to use a different set entirely.

### Data

Every `*.ron` file in a mod folder is [RON](https://docs.rs/ron) with the same
optional-everything shape, so the filename is documentation and nothing more —
the base game splits itself across `world.ron`, `calendar.ron`, `lands.ron`,
`buildings.ron`, `houses.ron` and `characters.ron` purely for readability.

Those files are **definitions**: read-only data the game only ever gains more
of. What *changes* — who rules what, what stands in each land, every
character's age and gold — is **state**, and lives in any `*.state.ron`
(`mods/base/start.state.ron` is where the world starts). That's the split a
save file runs along: a save is state, so it can be loaded against a game that
has grown new lands, buildings and characters since it was written.

State entries are an overlay keyed by id. Content the save never mentions
starts wherever its own mod says; state naming content that no longer exists is
dropped with a line in the chronicle rather than refused. Definitions are held
to a stricter standard — a dangling reference there stops the game, because it
means a mod is broken rather than merely old.

The calendar and the clock speeds are data too. Every month is the same length
and there are no leap days, so a year is just the two numbers multiplied.
`speeds` is simulated days per real second — `+` and `-` step through the list,
slowest first, and the game starts on the first entry:

```ron
// mods/slow-and-short/calendar.ron
(
    calendar: (days_per_month: 10, months_per_year: 5),
    speeds: [1, 2, 4],
)
```

Entries merge **by `id`**: same id replaces, new id appends. So a mod that
rebalances one building is three lines, and never has to fork the map:

```ron
// mods/rich-mills/buildings.ron
(buildings: [(id: "building-mill", name: "grain mill", gold_profit: 20)])
```

Cross-references are checked after everything has merged, so your mod may point
at a land or building another mod declares. An unknown section name is an error
rather than a silent no-op — a typo tells you about itself.

### Scripts

Kings Game uses [bevy_mod_scripting](https://github.com/makspll/bevy_mod_scripting)
for multi-language scripted modding. Scripts live in `assets/scripts/` and are
hot-reloadable. See `docs/scripting.md` for the full guide.

Two callback hooks are available:

- `on_day()` — fires every simulated day (every tick)
- `on_month()` — fires when the in-game month rolls over

```lua
-- assets/scripts/economy_overhaul.lua
function on_month()
    print("A new month begins")
end
```

Scripts access the ECS world via Bevy reflection. All game components
(`Character`, `Land`, `Kingdom`, etc.) are reflected and accessible from
script. See `docs/scripting.md` for the API.

**Cold path only.** Script callbacks fire once per event, not per entity.
Keep per-entity hot-loop computation in Rust systems or modifier tables.

```ron
// mods/rich-arryn/start.state.ron
(characters: [(id: "char-jon", age: 66, gold: 500)])
```

Use `rand()` rather than rolling your own randomness — it draws from the game's
seeded RNG, so a campaign still replays exactly from its seed.

Writes are collected and applied after every mod's hooks have run, so the
readable values don't shift under you mid-hook. The economy itself is no longer
a script — gold yield, levy and the monthly tax payout are computed in Rust on
the ECS each tick, so every ruler earns and raises without a mod. A script can
still read the same surface and call `add_character_gold` and the rest to layer
its own rules on top.

A script that fails to compile or throws is reported in the chronicle and then
disabled for the session. It never takes the game down with it.

The script surface is deliberately small: right now the simulation is a
calendar and a chronicle, so that is all there is to read and write. It grows
as the game does.

## License

MIT — see [LICENSE](LICENSE).

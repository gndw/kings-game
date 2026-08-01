# kings-game

A small grand-strategy sandbox in Rust: a hand-drawn island of three realms,
a 360-day calendar, and a chronicle that writes itself one simulated day at a
time. Bevy draws it, [hecs](https://docs.rs/hecs) simulates it.

## Running

```sh
make play              # release build — the one you actually play
make run               # debug build with dynamic linking
make play SEED=1066    # a specific campaign
make check             # fmt + clippy + tests
```

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
`buildings.ron`, `houses.ron`, `characters.ron` and `kingdoms.ron` purely for
readability.

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

A mod folder may also hold any number of `*.rhai` files
([Rhai](https://rhai.rs)). Define `on_startup`, `on_day`, `on_month`, any of
them, or none. `on_startup` fires once before the first tick — that's where the
base scripts publish a ruler's levy and monthly income, so a new game opens on
real numbers instead of zeroes. `on_month` only ever fires on day 1, so it needs
no date check of its own:

```rhai
// mods/plague/on_month.rhai
fn on_month(ctx) {
    if ctx.month == 6 && ctx.rand() < 0.05 {
        ctx.add_chronicle("A sickness takes the holdings.");
    }
}
```

As with the data files, the filename is documentation — each `*.rhai` compiles
on its own, so split a mod however reads best. The base game names each script
after what it does (`character_levy.rhai`, `character_gold.rhai`); one file with both
hooks works exactly the same.

What `ctx` can read:

| | |
|---|---|
| `year` `month` `day` `tick` | the clock |
| `land` | the selected land's id, or `""` |
| `player` | the player's character id |
| `characters` | every character id, in data order |
| `gold(id)` `levy(id)` | that character's resources, as of the start of the tick |
| `kingdoms` | every kingdom id, in data order |
| `kingdom_leader(kid)` | the character ruling it, or `""` |
| `kingdom_lands(kid)` `land_buildings(lid)` | what a realm holds, and what stands in a land |
| `building_levy(bid)` `building_gold_profit(bid)` `building_gold_upkeep(bid)` | what one building is worth |

What it can do:

| | |
|---|---|
| `rand()` | uniform in `[0, 1)`, from the seeded RNG |
| `add_chronicle(line)` | write a line to the chronicle |
| `add_character_gold(id, n)` | add to (or, negative, take from) a treasury |
| `set_character_levy(id, n)` | set a character's raised troops |
| `set_character_gold_yield(id, n)` | set a character's gold profit per month |

Gold and levy belong to characters, not to the player — the player is just an
id, and every ruler runs on the same rules. Who counts as a ruler is not a rule
the engine knows: the base scripts walk `kingdoms`, keep the ones whose leader
is the character in hand, and sum the buildings. Lead no kingdom and the sum is
zero. Change that loop and you change the rule.

Starting values live in the data, so a mod can hand someone a treasury:

```ron
// mods/rich-arryn/characters.ron
(characters: [(id: "char-jon", name: "jon", house_id: "house-arryn", age: 66, gold: 500)])
```

Use `rand()` rather than rolling your own randomness — it draws from the game's
seeded RNG, so a campaign still replays exactly from its seed.

Writes are collected and applied after every mod's hooks have run, so the
readable values don't shift under you mid-hook. The economy is itself just a
script — `mods/base/character_levy.rhai` sets levies and `mods/base/character_gold.rhai`
collects taxes on the first. Replace them by shipping a folder sorted after
`base`, or delete them and nobody earns anything.

A script that fails to compile or throws is reported in the chronicle and then
disabled for the session. It never takes the game down with it.

The script surface is deliberately small: right now the simulation is a
calendar and a chronicle, so that is all there is to read and write. It grows
as the game does.

## License

MIT — see [LICENSE](LICENSE).

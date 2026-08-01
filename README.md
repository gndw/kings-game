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

Load a savefile at startup:

```sh
cargo run --release -- --load saves/quicksave.save.ron
```

Pick your character on a new game:

```sh
cargo run --release -- --player-character-id char-tywin
```

Needs a Rust toolchain (edition 2024) and X11 dev libraries. Bevy is built
without wayland, audio, or 3d.

## Controls

| Key | |
|---|---|
| `space` | pause / unpause |
| `+` / `-` | simulation speed |
| `F5` | quicksave |
| `F9` | quickload |
| `q`, `esc` | quit |

## Saves

Saves are plain RON files in `saves/`. F5 quicksaves, F9 quickloads.
Load at startup with `--load <path>`.

The save stores **game state only** — building/house/character templates
are reloaded from `map.ron` on every load. New content added to the map
file between sessions is picked up automatically:

- New buildings become available as templates (not auto-placed)
- New characters enter the world at their definition age
- Tweaked stats (gold, levy) take effect immediately
- Removed content is dropped cleanly

## Modding

The map is plain [RON](https://docs.rs/ron) in `assets/map.ron` — borders are
polylines, edit and restart. Point `KINGS_MAP` at your own file to use it
instead.

## License

MIT — see [LICENSE](LICENSE).

# Decisions

Standing decisions for this project. Check here before designing something;
append a new section when a decision is made.

## Views are data, logic is Rhai

`src/mods/view.rs` is a snapshot of the world for scripts to read. Every
`*View` struct mirrors its counterpart — `BuildingView` matches
`content::Building`, `LandView` matches `state::LandState`, and so on: same
fields, same types, copied straight across.

Views hold no derived values and no rules. No sums, no "profit minus upkeep",
no filtering by who rules what. If a number isn't stored on the counterpart,
it doesn't belong in the view.

Anything that decides how the game behaves lives in a `.rhai` file under
`mods/`, where it can be modded. Cumulative gold is summed in
`character_gold.rhai`; levy in `character_levy.rhai`. Rust exposes the
individual values; the script decides what they add up to.

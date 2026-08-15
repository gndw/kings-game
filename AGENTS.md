# AGENTS.md

## Architecture

`docs/architecture.md` maps how the code is put together (layers, the data
load pipeline, the ECS world, the sim loop, UI, key invariants, a file map).
**Refer to it before any structural change** — a new entity kind, schedule,
data section, or refactoring across a layer. It is the *what*; the decision
doc is the *why*. If your change alters the structure that file describes
(component shape, a relationship, a schedule, the load passes, an invariant),
**update the relevant section in the same change** — a stale architecture doc
is worse than none.

## Decisions

`docs/decision.md` holds the standing decisions for this project. Read it
before designing anything; append a new section when a decision is made.

## Tests

Don't add tests by default — no `#[test]` blocks, no `tests/` scaffolding,
no testing-only deps. The user iterates fast and CI handles runs; bevy cold
compile makes a one-off smoke test uneconomical. Write tests only when the
user explicitly asks for them.

## Building and running

You may run `cargo check` after making changes to confirm syntax and types
are correct. Do not run `cargo build`, `cargo run`, `cargo test`, or `make`
— the user builds, runs, and tests the project themselves.

Make the edits, say what changed, and stop. If a change is unverified because
check failed or couldn't be run, say so plainly instead of implying it works.

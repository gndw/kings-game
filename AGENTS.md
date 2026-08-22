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
are correct, and `cargo run` to verify the project actually works (it
implies a build, which is fine — but don't invoke `cargo build` on its
own). Do not run `cargo test` or `make` — the user handles testing and
other build tooling themselves.

Be mindful that `cargo run` on a bevy app will open a window and block. For
non-visual smoke checks prefer `cargo check`; reach for `cargo run` only
when you genuinely need to confirm the program boots and behaves as
expected, and stop the process when you've seen enough.

Make the edits, say what changed, and stop. If a change is unverified because
check failed or couldn't be run, say so plainly instead of implying it works.

//! The simulation's entity-component model and the world it lives in.
//!
//! The entities live directly in Bevy's App world — [`Ctx`](crate::ctx::Ctx)
//! holds only the session state that isn't an entity (rng, chronicles, the
//! player id, the current selection). Reads go through Bevy `Query` system
//! params (the UI) or `&mut World` free functions (sim logic, which mixes
//! component and resource access and so runs as exclusive systems).
//!
//! - **House** entities: [`StringId`], [`House`], [`HouseName`].
//! - **Character** entities: [`StringId`], [`Character`], [`CharacterName`],
//!   [`CharacterAge`], [`CharacterGold`], [`CharacterLevy`],
//!   [`CharacterGoldYield`], [`HouseOf`], maybe [`Leads`].
//! - **Land** entities: [`StringId`], [`Land`], [`LandName`], [`LandBorders`],
//!   [`LandHolding`], [`Built`], maybe [`HeldBy`].
//! - **Kingdom** entities: [`StringId`], [`Kingdom`], [`LedBy`],
//!   [`Seat`], [`Holds`] (auto-maintained from each land's [`HeldBy`]).
//!
//! Building *definitions* are not entities — they are a read-only roster held
//! as the [`Buildings`](crate::resources::buildings::Buildings) resource; lands
//! keep the ids of what's built in [`Built`].
//!
//! Load-time [`Content`](crate::content::Content) — the merged definitions with
//! the starting state overlaid (still `IndexMap`-based: the deserialization,
//! merge and reconcile target) — is consumed by [`populate`] once, in
//! [`Ctx::new_game`](crate::ctx::Ctx::new_game); afterwards it is gone and the
//! ECS is the whole world.
//!
//! Two invariants carried over from the `IndexMap` model:
//!
//! - **Every game entity carries a [`StringId`]** — the id its RON data and save
//!   address it by. The Rhai script ABI is string ids and does not change.
//! - **Read order is Bevy archetype order**, which within one archetype is spawn
//!   order. Each kind (houses, characters, lands, kingdoms) is a single
//!   archetype, so a `Query` over e.g. `(&StringId, &Land)` yields lands in the
//!   order [`populate`] spawned them.
//!
//! A [`Registry`] resource maps `StringId → Entity` for O(1) lookup, the role
//! the `IndexMap` keys once played. Reading the registry and then mutating the
//! entity it points at is the standard two-step: pull the (cheap, `Copy`)
//! `Entity` out, drop the borrow, then touch the entity.
//!
//! Definitions live in one file per entity kind ([`character`], [`house`],
//! [`kingdom`], [`land`]); the shared spine — [`StringId`], [`Registry`],
//! [`populate`] — is in [`ecs`] and re-exported here as one flat namespace.

pub mod character;
pub mod ecs;
pub mod house;
pub mod kingdom;
pub mod land;

pub use character::*;
pub use ecs::*;
pub use house::*;
pub use kingdom::*;
pub use land::*;

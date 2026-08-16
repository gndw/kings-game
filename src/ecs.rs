//! The simulation's entity-component model and the world it lives in.
//!
//! The entities live directly in Bevy's App world — [`Ctx`](crate::ctx::Ctx)
//! holds only the session state that isn't an entity (rng, the player id,
//! the current selection; the chronicle log is its own resource). Reads go
//! through Bevy `Query` system params (the UI) or `&mut World` free
//! functions (sim logic, which mixes component and resource access and so
//! runs as exclusive systems).
//!
//! - **House** entities: [`StringId`], [`House`], [`HouseName`].
//! - **Character** entities: [`StringId`], [`Character`], [`CharacterName`],
//!   [`CharacterDateOfBirth`], [`CharacterGold`], [`CharacterLevy`],
//!   [`CharacterGoldYield`], [`CharacterOfHouse`], maybe [`CharacterLeads`],
//!   plus the six skill components ([`CharacterMartial`], [`CharacterProwess`],
//!   [`CharacterTreasury`], [`CharacterPrudence`], [`CharacterIntrigue`],
//!   [`CharacterFaith`], each `i32` 0..=20), plus family ties
//!   ([`CharacterHasFather`] / [`CharacterHasMother`] on the child,
//!   [`CharacterHasHusband`] on the wife with the auto-maintained
//!   [`CharacterHasWife`] target on the husband) and reverse Vecs
//!   (`CharacterHasFatheredChildren` / `CharacterHasMotheredChildren`) on the parents.
//!   Populated from `families.ron` after every character entity exists.
//! - **Land** entities: [`StringId`], [`Land`], [`LandName`], [`LandBorders`],
//!   [`LandHolding`], maybe [`LandHeldBy`] (auto-maintained from the holding
//!   kingdom's [`KingdomHold`]), plus a [`LandHasBuildings`] collection
//!   auto-maintained from each building's [`BuildingOnLand`].
//! - **Kingdom** entities: [`StringId`], [`Kingdom`], [`KingdomLedBy`],
//!   [`KingdomHold`] (its single held land).
//! - **Building** entities: [`StringId`], [`Building`], [`BuildingOf`] (a
//!   definition id into the [`BuildingDefs`](crate::resources::buildings::BuildingDefs)
//!   roster), [`BuildingOnLand`] (whose reverse [`LandHasBuildings`] sits on
//!   the land), [`BuildingStatus`] (active/inactive/building), and
//!   optionally [`BuildingConstructionDate`] (set on `BUILDING` instances
//!   until the date passes the def's `construction_time`).
//! - **Road** entities: [`StringId`], [`Road`], [`RoadPoints`] (the
//!   polyline), [`RoadBetweenLands`] (the two lands it joins — plain
//!   `Vec<Entity>`, not a Bevy relationship, since roads are baked at
//!   populate time and never change). Drawn as dashed lines by
//!   [`road_graphic`](crate::map::components::road_graphic).
//!
//! Building *definitions* are not entities — they are a read-only roster held
//! as the [`BuildingDefs`](crate::resources::buildings::BuildingDefs) resource;
//! each built building *instance* is an entity that points at its definition.
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
//!   order. Each kind (houses, characters, lands, buildings, kingdoms) is a single
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

pub mod army;
pub mod building;
pub mod character;
pub mod courtier;
pub mod ecs;
pub mod house;
pub mod kingdom;
pub mod land;
pub mod marching;
pub mod road;
pub mod siege;
pub mod war;

pub use army::*;
pub use building::*;
pub use character::*;
pub use courtier::*;
pub use ecs::*;
pub use house::*;
pub use kingdom::*;
pub use land::*;
pub use road::*;
pub use siege::*;
pub use war::*;

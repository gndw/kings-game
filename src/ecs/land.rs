//! Land entities: the map's territories.
//!
//! A land carries the [`Land`] marker plus [`LandName`], [`LandBorders`],
//! [`LandHolding`], a [`Built`] building list, and a [`HeldBy`] link to the
//! kingdom that holds it.

use super::kingdom::Holds;
use bevy::ecs::entity::Entity;
use bevy::prelude::Component;

/// A land. Name in [`LandName`], outline in [`LandBorders`], seat of power in
/// [`LandHolding`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Land;

/// A land's name.
#[derive(Component, Debug, Clone)]
pub struct LandName(pub String);

/// A land's polygon outline.
#[derive(Component, Debug, Clone)]
pub struct LandBorders(pub Vec<(f64, f64)>);

/// A land's seat of power.
#[derive(Component, Debug, Clone, Copy)]
pub struct LandHolding(pub (f64, f64));

/// What stands in a land: the ids of the buildings built there. State, not
/// content — it changes in play and belongs in a save. Looked up against the
/// [`Buildings`](crate::resources::buildings::Buildings) resource to render.
#[derive(Component, Debug, Clone, Default)]
pub struct Built(pub Vec<String>);

/// The kingdom that holds a land. Points at a [`Kingdom`](super::Kingdom)
/// entity. A Bevy relationship component: inserting it auto-maintains
/// [`Holds`] on the kingdom.
#[derive(Component, Debug, Clone, Copy)]
#[relationship(relationship_target = Holds)]
pub struct HeldBy(pub Entity);

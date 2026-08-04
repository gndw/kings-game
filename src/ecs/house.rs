//! House entities: the families characters belong to and kingdoms rule through.

use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::{Component, Reflect};

/// A family. Characters belong to one; kingdoms are ruled through them. The
/// name lives in [`HouseName`].
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct House;

/// A house's name.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct HouseName(pub String);

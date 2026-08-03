//! House entities: the families characters belong to and kingdoms rule through.

use bevy::prelude::Component;

/// A family. Characters belong to one; kingdoms are ruled through them. The
/// name lives in [`HouseName`].
#[derive(Component, Debug, Clone, Copy)]
pub struct House;

/// A house's name.
#[derive(Component, Debug, Clone)]
pub struct HouseName(pub String);

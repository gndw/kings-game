//! The chronicle log: lines appended as the game runs. An ECS resource,
//! seeded in `main` with the opening line; the UI reads the tail of it.

use bevy::prelude::Resource;

#[derive(Default, Resource)]
pub struct Chronicles(pub Vec<String>);

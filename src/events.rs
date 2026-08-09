//! ECS events shared across the game.

use bevy::prelude::*;

/// What just happened to a building, from the perspective of the
/// [`OnBuildingUpdated`] event's observer. `Constructed` is reserved for
/// future code paths that build a building already active; the current
/// `construct_building` command does not fire the event (the building is
/// inactive at spawn time), and the daily `construction` system fires
/// [`BuildingUpdateKind::Updated`] when a building transitions to `Active`.
#[derive(Event)]
pub struct OnBuildingUpdated {
    pub building: Entity,
    pub land: Entity,
    pub kind: BuildingUpdateKind,
}

pub enum BuildingUpdateKind {
    Constructed,
    Updated,
    Destroyed,
}

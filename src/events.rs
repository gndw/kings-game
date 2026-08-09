//! ECS events shared across the game.

use bevy::prelude::*;

/// Fired when something about a building changes. Lifecycle variants
/// ([`BuildingUpdateKind::Constructed`] / [`BuildingUpdateKind::Destroyed`])
/// fire from the construct / destroy commands; state variants
/// ([`BuildingUpdateKind::Raised`] / [`BuildingUpdateKind::Dismissed`]) fire
/// from the raise / dismiss army commands, one event per affected ACTIVE
/// building, after the structural change settles Bevy's relationship hooks.
#[derive(Event)]
pub struct OnBuildingUpdated {
    pub building: Entity,
    pub land: Entity,
    pub kind: BuildingUpdateKind,
}

pub enum BuildingUpdateKind {
    Constructed,
    Destroyed,
    Raised,
    Dismissed,
}

/// Fired by [`crate::commands::raise_army`] after the army bundle is spawned
/// and its building pools drained. Observers read `ArmyOnLand` / `ArmyName`
/// from `army` to position and label the icon.
#[derive(Event)]
pub struct OnArmyRaised {
    pub army: Entity,
}

/// Fired by [`crate::commands::dismiss_army`] after the army entity is
/// despawned. Observers use this to clean up the icon + label trio.
#[derive(Event)]
pub struct OnArmyDismiss {
    pub army: Entity,
}

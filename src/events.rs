//! ECS events shared across the game.

use bevy::prelude::*;

/// Fired when something about a building changes. Lifecycle variants
/// ([`BuildingUpdateKind::ConstructionStarted`] / [`BuildingUpdateKind::Constructed`] /
/// [`BuildingUpdateKind::Destroyed`]) fire from the construct / destroy commands and
/// the daily construction tick; state variants
/// ([`BuildingUpdateKind::Raised`] / [`BuildingUpdateKind::Dismissed`]) fire
/// from the raise / dismiss army commands, one event per affected ACTIVE
/// building, after the structural change settles Bevy's relationship hooks.
#[derive(Event)]
pub struct OnBuildingUpdated {
    pub building: Entity,
    pub land: Entity,
    pub kind: BuildingUpdateKind,
}

#[derive(Clone, Copy)]
pub enum BuildingUpdateKind {
    /// Fired by [`crate::commands::construct_building`] the moment a building
    /// is queued (status = `BUILDING`). Distinct from `Constructed`, which
    /// fires from the daily tick when the building finishes and flips to
    /// `ACTIVE` — two different chronicle-worthy moments (the construction
    /// starting, and the construction completing).
    ConstructionStarted,
    /// Fired by [`crate::game::construction`] the day a building's finish
    /// date passes and its status flips to `ACTIVE`.
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

/// Fired by [`crate::commands::marching`] after the per-hop marching
/// entities are spawned. `roads` is the count of hops on the route; `days`
/// is the summed `RoadDistanceDays` of those hops (the number the player
/// was quoted in the command palette). `from`/`to` are the route's two
/// ends — the army's current land and the picked destination.
#[derive(Event)]
pub struct OnMarchingOrdered {
    pub army: Entity,
    pub from: Entity,
    pub to: Entity,
    pub roads: u32,
    pub days: u32,
}

/// Fired by [`crate::game::marching::tick`] when an army hops onto the
/// target land of one of its marchings. `continuing` is `true` when the
/// army's queue still has a `Scheduled` marching whose `MarchingFromLand`
/// matches the land just arrived on (chain march — the chronicle should
/// mention the next hop); `false` when the queue is empty (route done —
/// army stands idle).
#[derive(Event)]
pub struct OnArmyArrived {
    pub army: Entity,
    pub from: Entity,
    pub to: Entity,
    pub continuing: bool,
}

/// Fired by [`crate::commands::lay_siege`] after the siege entity is
/// spawned and the army's status flips to `Sieging`.
#[derive(Event)]
pub struct OnSiegeLaid {
    pub army: Entity,
    pub land: Entity,
}

/// Fired by [`crate::game::siege::tick`] the moment a siege resolves at
/// 100% — buildings on the land flip to `Inactive`, `ArmyControlsLand`
/// lands on the army, and the army returns to `Idle`.
#[derive(Event)]
pub struct OnSiegeWon {
    pub army: Entity,
    pub land: Entity,
}

/// Fired by [`crate::commands::declare_war`] after the war entity is
/// spawned. `attacker` / `defender` are the two kingdoms; `casus_belli`
/// is the war's CB type (so the chronicle can pick the verb per shape —
/// "demanding its lands" for `Conquest`, future variants get their own
/// phrasing).
#[derive(Event)]
pub struct OnWarDeclared {
    pub attacker: Entity,
    pub defender: Entity,
    pub casus_belli: crate::ecs::war::WarCasusBelliType,
}

/// Fired by [`crate::commands::enforce_demands`] when a single demand is
/// resolved against a war. `target` is the demand's target kingdom (the
/// kingdom the `Take` demand was set against); `demand_type` lets the
/// chronicle pick a verb per shape.
#[derive(Event)]
pub struct OnDemandEnforced {
    pub demand_type: crate::ecs::war::WarDemandType,
    pub target: Entity,
}

/// Fired by [`crate::commands::enforce_demands`] when a war is despawned
/// after a demand resolution. `defender` is the war's defender kingdom
/// (the war's name in the chronicle is the target kingdom — "the war
/// over Riverrun ended"). Fires after `OnDemandEnforced` for the same
/// demand; the chronicle observer drops the second event because the
/// first already told the player what happened.
#[derive(Event)]
pub struct OnWarEnded {
    pub defender: Entity,
}

/// Fired by a command's validation when a player input is rejected
/// (unknown actor / not enough gold / no road route / …). The
/// `ui::error` module is the only observer: it pops a modal showing
/// `message` and switches the input layer to
/// [`InputLayer::ErrorPopup`](crate::resources::input_layer::InputLayer::ErrorPopup)
/// until the player dismisses it. Carries a single `message: String`
/// rather than a structured kind so commands can hand the player a
/// human-readable line verbatim without a code↔text table.
#[derive(Event)]
pub struct OnErrorOccured {
    pub message: String,
}

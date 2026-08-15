//! ECS events shared across the game.

use crate::resources::date::Date;
use bevy::prelude::*;

/// Fired when something about a building changes. Lifecycle variants come from
/// the construct/destroy commands and the daily construction tick; state
/// variants come from the raise/dismiss army commands.
#[derive(Event)]
pub struct OnBuildingUpdated {
    pub building: Entity,
    pub land: Entity,
    pub kind: BuildingUpdateKind,
}

#[derive(Clone, Copy)]
pub enum BuildingUpdateKind {
    /// Construction queued (status = `BUILDING`).
    ConstructionStarted,
    /// Building finished and flipped to `ACTIVE` (fired by the daily tick).
    Constructed,
    Destroyed,
    Raised,
    Dismissed,
}

/// Raised by `commands::raise_army` after the army bundle is spawned.
#[derive(Event)]
pub struct OnArmyRaised {
    pub army: Entity,
}

/// Dismissed by `commands::dismiss_army` after the army entity is despawned.
#[derive(Event)]
pub struct OnArmyDismiss {
    pub army: Entity,
}

/// Ordered by `commands::marching` after the per-hop marching entities are spawned.
#[derive(Event)]
pub struct OnMarchingOrdered {
    pub army: Entity,
    pub from: Entity,
    pub to: Entity,
    pub roads: u32,
    pub days: u32,
}

/// Fired by the marching tick when an army hops onto a target land.
/// `continuing: true` means the queue still has a Scheduled marching on this land.
#[derive(Event)]
pub struct OnArmyArrived {
    pub army: Entity,
    pub from: Entity,
    pub to: Entity,
    pub continuing: bool,
}

/// Laid by `commands::lay_siege` after the siege entity is spawned.
#[derive(Event)]
pub struct OnSiegeLaid {
    pub army: Entity,
    pub land: Entity,
}

/// Won by the siege tick the moment a siege resolves at 100%.
#[derive(Event)]
pub struct OnSiegeWon {
    pub army: Entity,
    pub land: Entity,
}

/// Declared by `commands::declare_war` after the war entity is spawned.
#[derive(Event)]
pub struct OnWarDeclared {
    pub attacker: Entity,
    pub defender: Entity,
    pub casus_belli: crate::ecs::war::WarCasusBelliType,
}

/// Enforced by `commands::enforce_demands` when a single demand is resolved.
#[derive(Event)]
pub struct OnDemandEnforced {
    pub demand_type: crate::ecs::war::WarDemandType,
    pub target: Entity,
}

/// Ended by `commands::enforce_demands` when a war is despawned after a demand.
#[derive(Event)]
pub struct OnWarEnded {
    pub defender: Entity,
}

/// Fired by a command's validation when a player input is rejected. The error
/// popup is the only observer.
#[derive(Event)]
pub struct OnErrorOccured {
    pub message: String,
}

/// Fired by the death-check system when a character's death roll resolves
/// `dead`. Sole publisher: `game::aging::on_day`. The inheriting observer
/// (`game::inheriting::on_character_died`) is the primary consumer.
#[derive(Event)]
pub struct OnCharacterDied {
    pub character: Entity,
    pub on_date: Date,
}

/// Fired by the inheriting system when it reassigns a kingdom's leader.
/// `to = None` and `relation = SuccessionRelation::Leaderless` mean the realm
/// has no heir under the current rules.
#[derive(Event)]
pub struct OnKingdomSucceeded {
    pub kingdom: Entity,
    pub from: Entity,
    pub to: Option<Entity>,
    pub relation: SuccessionRelation,
}

/// How the inheriting system resolved the succession — used only for the
/// chronicle label; the heir picker's branch is what determines it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuccessionRelation {
    EldestSon,
    MaleSibling,
    ElderOfHouse,
    Leaderless,
}

impl SuccessionRelation {
    pub fn label(self) -> &'static str {
        match self {
            SuccessionRelation::EldestSon => "eldest son",
            SuccessionRelation::MaleSibling => "male sibling",
            SuccessionRelation::ElderOfHouse => "elder of the house",
            SuccessionRelation::Leaderless => "leaderless",
        }
    }
}

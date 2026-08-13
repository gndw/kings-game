//! Chronicle generation: one observer per game event, each writing a single
//! narratively-flavored line to the [`Chronicles`](crate::resources::chronicle::Chronicles)
//! resource.
//!
//! The chronicle is the *story* of the realm — what happened, not what the
//! code did. Lines are past tense, third person, name lands by
//! [`LandName`](crate::ecs::LandName) and armies by
//! [`ArmyName`](crate::ecs::army::ArmyName); ids never appear in text, and
//! game-mechanic words like "active", "conquest", "Take enforced" are
//! replaced by their narrative equivalents.
//!
//! Events that today produce a chronicle line:
//!
//! - [`OnBuildingUpdated::ConstructionStarted`](crate::events::BuildingUpdateKind::ConstructionStarted) —
//!   "You began raising a Y at Z."
//! - [`OnBuildingUpdated::Constructed`](crate::events::BuildingUpdateKind::Constructed) —
//!   "The Y at Z is now in operation, its ... flowing into the realm's coffers."
//! - [`OnBuildingUpdated::Destroyed`](crate::events::BuildingUpdateKind::Destroyed) —
//!   "You tore down the Y at Z, its stones scattered to the winds."
//! - [`OnArmyRaised`](crate::events::OnArmyRaised) —
//!   "You mustered the Y at Z — N spears answering the call."
//! - [`OnArmyDismiss`](crate::events::OnArmyDismiss) —
//!   "You stood down the Y, its levy returning home to Z."
//! - [`OnMarchingOrdered`](crate::events::OnMarchingOrdered) —
//!   "You ordered the Y to march from A toward B. (D days by road.)"
//! - [`OnArmyArrived`](crate::events::OnArmyArrived) —
//!   "The Y arrived at B, having marched from A." / "...and pressed onward toward C."
//! - [`OnSiegeLaid`](crate::events::OnSiegeLaid) —
//!   "You laid siege to Z, your army sealing every road."
//! - [`OnSiegeWon`](crate::events::OnSiegeWon) —
//!   "After days of siege, your Y broke Z's walls and took the land."
//! - [`OnWarDeclared`](crate::events::OnWarDeclared) —
//!   "<attacker> declared war on <defender>, demanding its lands."
//! - [`OnDemandEnforced`](crate::events::OnDemandEnforced) —
//!   "You claimed the Kingdom of Y, taking the crown for your own."
//! - [`OnWarEnded`](crate::events::OnWarEnded) —
//!   "The war over Y ended."
//!
//! `Raised` / `Dismissed` per-building variants of [`OnBuildingUpdated`](crate::events::OnBuildingUpdated)
//! are ignored here — the chronicle line about raising / dismissing an army
//! comes from [`OnArmyRaised`](crate::events::OnArmyRaised) /
//! [`OnArmyDismiss`](crate::events::OnArmyDismiss), not from each drained
//! pool. The events still fire so
//! [`crate::game::yields::on_building_updated`] can keep the realm's
//! treasury in sync.
//!
//! ponytail: one observer per event, each a free function with Bevy
//! observer system params. The chronicle write is `chronicles.push(line)`;
//! every other read is `Query` / `Res`. No `&mut World` — Bevy 0.19
//! observers don't need it. The `PlayerCtx` system param resolves the
//! player character once per event (not once per observer-call — Bevy
//! shares the param across the observer batch), so the cost is the same
//! as a single `Res<Game>` read.

use crate::app::Game;
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyMaxLevy, ArmyName, ArmyOnLand, ArmyStatus};
use crate::ecs::building::BuildingOf;
use crate::ecs::character::{CharacterName, CharacterOfHouse};
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::KingdomHold;
use crate::ecs::land::LandName;
use crate::ecs::marching::{MarchingStatus, MarchingToLand};
use crate::ecs::war::{WarCasusBelliType, WarDemandType};
use crate::ecs::{Registry, StringId};
use crate::events::{
    BuildingUpdateKind, OnArmyArrived, OnArmyDismiss, OnArmyRaised, OnBuildingUpdated,
    OnDemandEnforced, OnMarchingOrdered, OnSiegeLaid, OnSiegeWon, OnWarDeclared, OnWarEnded,
};
use crate::resources::buildings::BuildingDefs;
use crate::resources::chronicle::Chronicles;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

// --- building-update dispatch --------------------------------------------
// Three observers each fire on the same `OnBuildingUpdated` event but
// match on `event.kind`. Bevy observers can't branch on payload fields,
// so the dispatch is `if !matches!(...)) { return; }` at the top of each.
// `Raised` / `Dismissed` are intentionally not observed here — the
// chronicle line about raising / dismissing an army comes from the
// army-level events.

pub fn on_construction_started(
    trigger: On<OnBuildingUpdated>,
    mut chronicles: ResMut<Chronicles>,
    building_of: Query<&BuildingOf>,
    land_names: Query<&LandName>,
    defs: Res<BuildingDefs>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    if !matches!(event.kind, BuildingUpdateKind::ConstructionStarted) {
        return;
    }
    let def_name = building_def_name(&building_of, &defs, event.building);
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "{} began raising a {def_name} at {land_str}.",
        player.short()
    ));
}

pub fn on_constructed(
    trigger: On<OnBuildingUpdated>,
    mut chronicles: ResMut<Chronicles>,
    building_of: Query<&BuildingOf>,
    land_names: Query<&LandName>,
    defs: Res<BuildingDefs>,
) {
    let event = trigger.event();
    if !matches!(event.kind, BuildingUpdateKind::Constructed) {
        return;
    }
    let def_id = building_of
        .get(event.building)
        .map(|bo| bo.0.clone())
        .unwrap_or_default();
    let def_name = defs
        .get(&def_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| def_id.clone());
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "The {def_name} at {land_str} is now in operation, its {} flowing into the realm's coffers.",
        building_benefit(&def_id)
    ));
}

pub fn on_destroyed(
    trigger: On<OnBuildingUpdated>,
    mut chronicles: ResMut<Chronicles>,
    building_of: Query<&BuildingOf>,
    land_names: Query<&LandName>,
    defs: Res<BuildingDefs>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    if !matches!(event.kind, BuildingUpdateKind::Destroyed) {
        return;
    }
    let def_name = building_def_name(&building_of, &defs, event.building);
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "{} tore down the {def_name} at {land_str}, its stones scattered to the winds.",
        player.short()
    ));
}

// --- army events ---------------------------------------------------------

pub fn on_army_raised(
    trigger: On<OnArmyRaised>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    army_on_land: Query<&ArmyOnLand>,
    army_levy: Query<&ArmyLevy>,
    army_max_levy: Query<&ArmyMaxLevy>,
    army_status: Query<&ArmyStatus>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let army = trigger.event().army;
    let name = army_name
        .get(army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    let land = army_on_land
        .get(army)
        .map(|a| land_name_of(&land_names, a.0))
        .unwrap_or_else(|_| "an unknown land".to_string());
    let levy = army_levy.get(army).map(|l| l.0).unwrap_or(0);
    let max = army_max_levy.get(army).map(|m| m.0).unwrap_or(levy);
    let is_raising = army_status
        .get(army)
        .map(|s| *s == ArmyStatus::Raising)
        .unwrap_or(false);
    // A `Raising` army starts with `ArmyLevy = 0`; its `ArmyMaxLevy` is
    // the formation target. Phrase the line as "raising up to N" rather
    // than "0 spears answering the call", which would read as a
    // no-troops army. The full levy lands in a later line once the
    // formation tick flips status to `Idle` — but that transition isn't
    // its own chronicle event, so the muster line is the player's only
    // read of the size at raise time.
    if is_raising && levy == 0 {
        chronicles.0.push(format!(
            "{} began raising the {name} at {land} — up to {max} spears gathering for the muster.",
            player.short()
        ));
    } else {
        chronicles.0.push(format!(
            "{} mustered the {name} at {land} — {levy} spears answering the call.",
            player.short()
        ));
    }
}

pub fn on_army_dismiss(
    trigger: On<OnArmyDismiss>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    army_kingdom: Query<&ArmyBelongsToKingdom>,
    kingdom_hold: Query<&KingdomHold>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let army = trigger.event().army;
    let name = army_name
        .get(army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    // The "home" land is the army's kingdom's held land — the levy always
    // returns there on dismiss, regardless of where the army currently
    // stands (mirrors `commands::dismiss_army`).
    let home = army_kingdom
        .get(army)
        .ok()
        .and_then(|abk| kingdom_hold.get(abk.0).ok())
        .map(|kh| land_name_of(&land_names, kh.0))
        .unwrap_or_else(|| "its home".to_string());
    chronicles.0.push(format!(
        "{} stood down the {name}, its levy returning home to {home}.",
        player.short()
    ));
}

// --- marching events -----------------------------------------------------

pub fn on_marching_ordered(
    trigger: On<OnMarchingOrdered>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let name = army_name
        .get(event.army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    let from_str = land_name_of(&land_names, event.from);
    let to_str = land_name_of(&land_names, event.to);
    chronicles.0.push(format!(
        "{} ordered the {name} to march from {from_str} toward {to_str}. ({days} days by road.)",
        player.short(),
        days = event.days
    ));
}

pub fn on_army_arrived(
    trigger: On<OnArmyArrived>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    army_has_marching: Query<&ArmyHasMarching>,
    marching_status: Query<&MarchingStatus>,
    marching_to_land: Query<&MarchingToLand>,
    land_names: Query<&LandName>,
) {
    let event = trigger.event();
    let name = army_name
        .get(event.army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    let from_str = land_name_of(&land_names, event.from);
    let to_str = land_name_of(&land_names, event.to);
    if event.continuing {
        // Look up the next scheduled marching's destination so the line
        // can name where the army is heading next.
        let next_target = army_has_marching
            .get(event.army)
            .ok()
            .and_then(|queue| {
                queue.iter().find_map(|m_e| {
                    let status = marching_status.get(m_e).ok()?;
                    if *status != MarchingStatus::Scheduled {
                        return None;
                    }
                    marching_to_land.get(m_e).ok().map(|to| to.0)
                })
            });
        match next_target {
            Some(next_land) => {
                let next_str = land_name_of(&land_names, next_land);
                chronicles.0.push(format!(
                    "The {name} reached {to_str} and pressed onward toward {next_str}."
                ));
            }
            None => {
                chronicles.0.push(format!(
                    "The {name} arrived at {to_str}, having marched from {from_str}."
                ));
            }
        }
    } else {
        chronicles.0.push(format!(
            "The {name} arrived at {to_str}, having marched from {from_str}."
        ));
    }
}

// --- siege events --------------------------------------------------------

pub fn on_siege_laid(
    trigger: On<OnSiegeLaid>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let name = army_name
        .get(event.army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "{} laid siege to {land_str}, your {name} sealing every road.",
        player.short()
    ));
}

pub fn on_siege_won(
    trigger: On<OnSiegeWon>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    land_names: Query<&LandName>,
) {
    let event = trigger.event();
    let name = army_name
        .get(event.army)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "Army".to_string());
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "After days of siege, your {name} broke {land_str}'s walls and took the land."
    ));
}

// --- war events ----------------------------------------------------------

pub fn on_war_declared(
    trigger: On<OnWarDeclared>,
    mut chronicles: ResMut<Chronicles>,
    kingdom_hold: Query<&KingdomHold>,
    land_names: Query<&LandName>,
    string_ids: Query<&StringId>,
) {
    let event = trigger.event();
    let attacker = kingdom_label(event.attacker, &kingdom_hold, &land_names, &string_ids);
    let defender = kingdom_label(event.defender, &kingdom_hold, &land_names, &string_ids);
    let phrase = match event.casus_belli {
        WarCasusBelliType::Conquest => "demanding its lands",
    };
    chronicles.0.push(format!(
        "{attacker} declared war on {defender}, {phrase}."
    ));
}

pub fn on_demand_enforced(
    trigger: On<OnDemandEnforced>,
    mut chronicles: ResMut<Chronicles>,
    kingdom_hold: Query<&KingdomHold>,
    land_names: Query<&LandName>,
    string_ids: Query<&StringId>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let target = kingdom_label(event.target, &kingdom_hold, &land_names, &string_ids);
    let line = match event.demand_type {
        WarDemandType::Take => format!(
            "{} claimed the Kingdom of {target}, taking the crown for your own.",
            player.short()
        ),
    };
    chronicles.0.push(line);
}

pub fn on_war_ended(
    trigger: On<OnWarEnded>,
    mut chronicles: ResMut<Chronicles>,
    kingdom_hold: Query<&KingdomHold>,
    land_names: Query<&LandName>,
    string_ids: Query<&StringId>,
) {
    let defender = trigger.event().defender;
    let target = kingdom_label(defender, &kingdom_hold, &land_names, &string_ids);
    chronicles.0.push(format!("The war over {target} ended."));
}

// --- helpers --------------------------------------------------------------

/// `PlayerCtx` is a Bevy `SystemParam` that resolves the player character
/// entity from the [`Game`] resource once per observer batch, then exposes
/// label helpers. Each observer that names the player takes this as a
/// system param; the Bevy scheduler shares one resolved instance across
/// the observer's system-param calls.
#[derive(SystemParam)]
pub struct PlayerCtx<'w, 's> {
    game: Res<'w, Game>,
    registry: Res<'w, Registry>,
    character_name: Query<'w, 's, &'static CharacterName>,
    character_house: Query<'w, 's, &'static CharacterOfHouse>,
    house_name: Query<'w, 's, &'static HouseName>,
}

impl<'w, 's> PlayerCtx<'w, 's> {
    /// The player character's id, or `None` in a torn world.
    fn entity(&self) -> Option<Entity> {
        self.registry.get(&self.game.ctx.player_character_id)
    }

    /// Short second-person pronoun for player-driven actions
    /// ("You mustered the army."). Reads nothing from the world —
    /// `Game` itself tells us the player is the actor; no need to walk
    /// the character chain.
    fn short(&self) -> &'static str {
        "You"
    }

    /// Full name + house for lines that should read like the chronicle
    /// book: `"Tywin of House Lannister"`. Falls back to `"you"` when the
    /// player character has no name or no house link.
    #[allow(dead_code)]
    fn full(&self) -> String {
        let Some(e) = self.entity() else {
            return "you".to_string();
        };
        let Ok(name) = self.character_name.get(e) else {
            return "you".to_string();
        };
        let suffix = self
            .character_house
            .get(e)
            .ok()
            .and_then(|coh| self.house_name.get(coh.0).ok())
            .map(|hn| format!(" of {}", hn.0));
        match suffix {
            Some(s) => format!("{}{}", name.0, s),
            None => name.0.clone(),
        }
    }
}

/// The display name of a building instance's def. Reads `BuildingOf` off
/// the instance, then looks the def up in the resource. Falls back to
/// the def id if the roster can't find it.
fn building_def_name(
    building_of: &Query<&BuildingOf>,
    defs: &BuildingDefs,
    building: Entity,
) -> String {
    let def_id = building_of
        .get(building)
        .map(|bo| bo.0.clone())
        .unwrap_or_default();
    defs.get(&def_id)
        .map(|d| d.name.clone())
        .unwrap_or(def_id)
}

/// The display name of a land, falling back to `"an unknown land"` so
/// the chronicle never says `"at "` with nothing after it.
fn land_name_of(land_names: &Query<&LandName>, land: Entity) -> String {
    land_names
        .get(land)
        .map(|ln| ln.0.clone())
        .unwrap_or_else(|_| "an unknown land".to_string())
}

/// A kingdom's display label = its held land's name, with the kingdom
/// id as a last-resort fallback. Used by war / demand lines where naming
/// the realm (the seat of power) reads better than naming the land.
fn kingdom_label(
    kingdom: Entity,
    kingdom_hold: &Query<&KingdomHold>,
    land_names: &Query<&LandName>,
    string_ids: &Query<&StringId>,
) -> String {
    if let Ok(kh) = kingdom_hold.get(kingdom)
        && let Ok(ln) = land_names.get(kh.0)
    {
        return ln.0.clone();
    }
    string_ids
        .get(kingdom)
        .map(|s| s.0.clone())
        .unwrap_or_else(|_| "an unknown kingdom".to_string())
}

/// Per-def "benefit" phrasing for a finished-construction chronicle line.
/// Distinct per kind so a granary says "stores filling the realm's
/// coffers" and a barracks says "soldiers swelling the levy". The def
/// roster carries no `benefit` blurb today, so this falls back to a
/// generic "work" — author per-def phrasings when the building catalogue
/// grows the field. ponytail: per-def `benefit` would be the obvious
/// next field; not adding it now because no def uses it.
fn building_benefit(def_id: &str) -> &'static str {
    match def_id {
        "granary" => "stores filling the realm's coffers",
        "barracks" => "soldiers swelling the levy",
        "market" => "trade flowing into the realm's coffers",
        "farm" => "harvests feeding the realm",
        "mine" => "ore feeding the forges",
        _ => "work beginning",
    }
}

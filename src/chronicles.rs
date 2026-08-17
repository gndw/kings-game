//! Chronicle generation: one observer per game event, each writing a single
//! narratively-flavored line to `Chronicles`.
//!
//! Past tense, third person, names lands/armies, never ids or game-mechanic
//! words. `Raised`/`Dismissed` per-building variants are absorbed here — the
//! army-level line covers them.

use crate::app::Game;
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyMaxLevy, ArmyName, ArmyOnLand, ArmyStatus};
use crate::ecs::building::BuildingOf;
use crate::ecs::character::{CharacterDateOfBirth, CharacterName, CharacterOfHouse};
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::KingdomHold;
use crate::ecs::land::LandName;
use crate::ecs::marching::{MarchingStatus, MarchingToLand};
use crate::ecs::war::{WarCasusBelliType, WarDemandType};
use crate::ecs::{Registry, StringId};
use crate::observers::{
    BuildingUpdateKind, OnArmyArrived, OnArmyDismiss, OnArmyRaised, OnBuildingUpdated,
    OnCharacterDied, OnDemandEnforced, OnEventResolved, OnGoldGifted, OnKingdomSucceeded,
    OnMarchingOrdered, OnSiegeLaid, OnSiegeWon, OnWarDeclared, OnWarEnded,
};
use crate::helper::age_helper::age;
use crate::game::presenting_event::EventDeck;
use crate::resources::buildings::BuildingDefs;
use crate::resources::event_scripts::EventScripts;
use crate::resources::calendar::Calendar;
use crate::resources::chronicle::Chronicles;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

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
    let def_id = building_of.get(event.building).map(|bo| bo.0.clone()).unwrap_or_default();
    let def_name = defs.get(&def_id).map(|d| d.name.clone()).unwrap_or_else(|| def_id.clone());
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
    let name = army_name.get(army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
    let land = army_on_land.get(army).map(|a| land_name_of(&land_names, a.0)).unwrap_or_else(|_| "an unknown land".to_string());
    let levy = army_levy.get(army).map(|l| l.0).unwrap_or(0);
    let max = army_max_levy.get(army).map(|m| m.0).unwrap_or(levy);
    let is_raising = army_status.get(army).map(|s| *s == ArmyStatus::Raising).unwrap_or(false);
    // A `Raising` army starts with `ArmyLevy = 0`; phrase as "raising up to N" rather than "0 spears".
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
    let name = army_name.get(army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
    // The "home" land is the army's kingdom's held land — the levy always returns there.
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

pub fn on_marching_ordered(
    trigger: On<OnMarchingOrdered>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let name = army_name.get(event.army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
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
    let name = army_name.get(event.army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
    let from_str = land_name_of(&land_names, event.from);
    let to_str = land_name_of(&land_names, event.to);
    if event.continuing {
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

pub fn on_siege_laid(
    trigger: On<OnSiegeLaid>,
    mut chronicles: ResMut<Chronicles>,
    army_name: Query<&ArmyName>,
    land_names: Query<&LandName>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let name = army_name.get(event.army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
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
    let name = army_name.get(event.army).map(|n| n.0.clone()).unwrap_or_else(|_| "Army".to_string());
    let land_str = land_name_of(&land_names, event.land);
    chronicles.0.push(format!(
        "After days of siege, your {name} broke {land_str}'s walls and took the land."
    ));
}

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

pub fn on_character_died(
    trigger: On<OnCharacterDied>,
    mut chronicles: ResMut<Chronicles>,
    character_names: Query<(&CharacterName, &CharacterDateOfBirth)>,
    calendar: Res<Calendar>,
) {
    let event = trigger.event();
    let Ok((name, dob)) = character_names.get(event.character) else {
        return;
    };
    let age_at_death = age(&dob.0, &event.on_date, &calendar);
    let year = event.on_date.year;
    chronicles.0.push(format!(
        "{} died at the age of {age_at_death} in year {year}",
        name.0
    ));
}

pub fn on_kingdom_succeeded(
    trigger: On<OnKingdomSucceeded>,
    mut chronicles: ResMut<Chronicles>,
    character_names: Query<&CharacterName>,
    kingdom_hold: Query<&KingdomHold>,
    land_names: Query<&LandName>,
    string_ids: Query<&StringId>,
) {
    use crate::observers::SuccessionRelation;
    let event = trigger.event();
    let realm = kingdom_label(event.kingdom, &kingdom_hold, &land_names, &string_ids);
    let line = match event.to {
        Some(new_e) => {
            let new_name = character_names
                .get(new_e)
                .map(|n| n.0.clone())
                .unwrap_or_else(|_| "an unknown heir".to_string());
            match event.relation {
                SuccessionRelation::EldestSon => format!(
                    "The realm of {realm} passed from the late ruler to their child {new_name}."
                ),
                SuccessionRelation::MaleSibling => format!(
                    "The realm of {realm} passed from the late ruler to their brother {new_name}."
                ),
                SuccessionRelation::ElderOfHouse => format!(
                    "With no close kin to inherit, the realm of {realm} passed to the elder of the house, {new_name}."
                ),
                // Unreachable in practice — inheriting sets `to = None` whenever relation is Leaderless.
                SuccessionRelation::Leaderless => format!(
                    "The realm of {realm} found no close kin — the crown passed to {new_name}, the elder of the house."
                ),
            }
        }
        None => format!(
            "The realm of {realm} has no heir — it stands leaderless, awaiting a claimant."
        ),
    };
    chronicles.0.push(line);
}

/// Resolves the player character once per observer batch and exposes label helpers.
#[derive(SystemParam)]
pub struct PlayerCtx<'w, 's> {
    game: Res<'w, Game>,
    registry: Res<'w, Registry>,
    character_name: Query<'w, 's, &'static CharacterName>,
    character_house: Query<'w, 's, &'static CharacterOfHouse>,
    house_name: Query<'w, 's, &'static HouseName>,
}

impl<'w, 's> PlayerCtx<'w, 's> {
    fn entity(&self) -> Option<bevy::ecs::entity::Entity> {
        self.game.ctx.player_character_id.as_deref().and_then(|id| self.registry.get(id))
    }

    fn short(&self) -> &'static str {
        "You"
    }

    #[allow(dead_code)]
    fn full(&self) -> String {
        let Some(e) = self.entity() else { return "you".to_string(); };
        let Ok(name) = self.character_name.get(e) else { return "you".to_string(); };
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

/// Display name of a building instance's def.
fn building_def_name(
    building_of: &Query<&BuildingOf>,
    defs: &BuildingDefs,
    building: bevy::ecs::entity::Entity,
) -> String {
    let def_id = building_of.get(building).map(|bo| bo.0.clone()).unwrap_or_default();
    defs.get(&def_id).map(|d| d.name.clone()).unwrap_or(def_id)
}

/// Display name of a land, falling back to `"an unknown land"`.
fn land_name_of(land_names: &Query<&LandName>, land: bevy::ecs::entity::Entity) -> String {
    land_names.get(land).map(|ln| ln.0.clone()).unwrap_or_else(|_| "an unknown land".to_string())
}

/// A kingdom's display label = its held land's name, with kingdom id as last-resort fallback.
fn kingdom_label(
    kingdom: bevy::ecs::entity::Entity,
    kingdom_hold: &Query<&KingdomHold>,
    land_names: &Query<&LandName>,
    string_ids: &Query<&StringId>,
) -> String {
    if let Ok(kh) = kingdom_hold.get(kingdom)
        && let Ok(ln) = land_names.get(kh.0)
    {
        return ln.0.clone();
    }
    string_ids.get(kingdom).map(|s| s.0.clone()).unwrap_or_else(|_| "an unknown kingdom".to_string())
}

/// Per-def benefit phrasing for the finished-construction line.
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

pub fn on_gold_gifted(
    trigger: On<OnGoldGifted>,
    mut chronicles: ResMut<Chronicles>,
    character_names: Query<&CharacterName>,
    player: PlayerCtx,
) {
    let event = trigger.event();
    let from_name = character_names
        .get(event.from)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "someone".to_string());
    let to_name = character_names
        .get(event.to)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "someone".to_string());
    let actor = player.short();
    let line = if event.from == player.entity().unwrap_or(event.from) {
        // Player-driven gift — speak as "You".
        format!("{actor} gifted {} gold to {to_name}.", event.amount)
    } else {
        format!("{from_name} gifted {} gold to {to_name}.", event.amount)
    };
    chronicles.0.push(line);
}

/// Event-resolution chronicle. Runs before
/// [`crate::game::presenting_event::on_event_resolved`] (registration order
/// in `main.rs`) so it still sees `pending = Some` before the resolver
/// clears it.
///
/// Source of the per-choice decline line, in priority order:
///
/// 1. The choice's `chronicle` field (set by the script's `choices()`
///    return value).
/// 2. The event's `decline()` function.
/// 3. Generic fallback: `"You turned {0.name} away."`.
///
/// Templates may use `{N.name}` placeholders; the observer substitutes
/// them with the Nth character's display name from `pending.characters`.
/// Missing indices fall back to `"a stranger"` (via `substitute_names`).
///
/// Gold-moving choices are already chronicled by [`on_gold_gifted`] (the
/// resolver calls `transfer_with_gold_memory` from the script, which fires
/// `OnGoldGifted`). The script is responsible for calling `ctx.log(...)`
/// if it wants custom gold-chronicle text.
pub fn on_event_resolved(
    trigger: On<OnEventResolved>,
    deck: Res<EventDeck>,
    scripts: Res<EventScripts>,
    // ponytail: a `&World` here would force `read_all` access and collide
    // with the `ResMut<Chronicles>` below. Use a query that names the
    // specific components we read off each character instead.
    characters: Query<(
        &crate::ecs::StringId,
        &crate::ecs::CharacterName,
        Option<&crate::ecs::CharacterOfHouse>,
        &crate::ecs::CharacterLevy,
        &crate::ecs::CharacterGold,
        &crate::ecs::CharacterIsAlive,
    )>,
    house_string_ids: Query<&crate::ecs::StringId>,
    mut chronicles: ResMut<Chronicles>,
) {
    let event = trigger.event();
    let Some(pending) = deck.pending.as_ref() else {
        return;
    };
    let Some(ev) = scripts.events.get(pending.def_index) else {
        return;
    };
    // Build character view maps for `{N.name}` substitution.
    let character_views: Vec<rhai::Map> = pending
        .characters
        .iter()
        .map(|e| crate::script_ctx::character_view_from_queries(*e, &characters, &house_string_ids))
        .collect();
    let first_name = character_views
        .first()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.clone().into_string().ok())
        .unwrap_or_else(|| "a stranger".to_string());

    let line = match event.choice {
        None => format!("You dismissed {first_name} without a word."),
        Some(idx) => {
            let per_choice = ev
                .call_choices(&scripts.engine)
                .ok()
                .and_then(|rows| rows.into_iter().nth(idx))
                .and_then(|row| row.chronicle);
            let template = per_choice
                .or_else(|| ev.call_decline(&scripts.engine))
                .unwrap_or_else(|| "You turned {0.name} away.".to_string());
            crate::script_ctx::substitute_names(&template, &character_views)
        }
    };
    chronicles.0.push(line);
}

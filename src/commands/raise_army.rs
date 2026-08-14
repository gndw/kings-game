//! The raise-army command: spawn an `Army` entity on a land the actor rules.
//!
//! One step (pick a ruled land). The army starts in `ArmyStatus::Raising` with
//! `ArmyLevy = 0` and `ArmyMaxLevy = sum of available BuildingLevy pools on the
//! land`. The per-day raising tick accretes up to 20 levy per raised building
//! per day until it reaches `ArmyMaxLevy`, then flips to `Idle`.

use super::core::{
    available_levy, error, next_id, picker_row, ruled_lands, set_row_selected, BaseCommand,
    NAME_COLOR, STAT_COLOR, STAT_DIM,
};
use crate::app::Game;
use crate::ecs::army::{
    Army, ArmyBelongsToKingdom, ArmyLevy, ArmyMaxLevy, ArmyName, ArmyOnLand, ArmyStatus,
};
use crate::ecs::building::{BuildingIsRaised, BuildingStatus};
use crate::ecs::{
    CharacterLeads, CharacterOfHouse, HouseName, LandHasArmies, LandHeldBy, LandHasBuildings,
    Registry, StringId,
};
use crate::events::OnArmyRaised;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

pub struct RaiseArmy;

impl BaseCommand for RaiseArmy {
    fn get_command_id(&self) -> &'static str {
        "command:raise_army"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        let command_pick = choices.iter().find(|(k, _)| k == "command").map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            let row = picker_row(
                world, parent, self.get_command_id(), None,
                "Raise Army", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let land_pick = choices.iter().find(|(k, _)| k == "land_id").map(|(_, v)| v.clone());
        if land_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone();
            let lands = ruled_lands(world, &actor);
            let mut entities = Vec::new();
            for (land_id, land_name) in lands {
                let land_e = world.resource::<Registry>().get(&land_id);
                let (pool, has_any) = land_e.map(|e| available_levy(world, e)).unwrap_or((0, false));
                let armies_here = land_e
                    .and_then(|e| world.get::<LandHasArmies>(e))
                    .map(|lha| lha.iter().count())
                    .unwrap_or(0);
                let pool_text = if has_any { pool.to_string() } else { String::new() };
                let pool_color = if has_any && pool > 0 { STAT_COLOR } else { STAT_DIM };
                let (name, name_color) = if !has_any || pool == 0 {
                    (format!("{land_name} (no levy)"), super::core::HINT_RED)
                } else {
                    (land_name.clone(), NAME_COLOR)
                };
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("land_id".to_string(), land_id)),
                    &name, name_color, None,
                    Some((&pool_text, pool_color)),
                    Some((&format!("{armies_here} here"), STAT_DIM)),
                );
                entities.push(row);
            }
            return (entities, false);
        }

        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let land_id = land_pick.as_deref().expect("step 1 reached without a land_id pick");
        raise(world, &actor, land_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

/// Spawn the army. Validates the actor rules the land, sums the available `BuildingLevy` pools
/// (refusing if none), creates the army in `Raising`, flags the contributing buildings with
/// `BuildingIsRaised`, fires `OnArmyRaised`.
fn raise(world: &mut World, actor: &str, land_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot raise on {land_id}: unknown actor"));
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return error(world, format!("cannot raise on {land_id}: no such land"));
    };

    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let land_kingdom = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    let kingdom_e = match (actor_kingdoms, land_kingdom) {
        (Some(ks), Some(lk)) if ks.contains(&lk) => lk,
        _ => {
            return error(world, format!("cannot raise on {land_id}: you don't rule that land"));
        }
    };

    let (max_levy, has_levy) = available_levy(world, land_e);
    if !has_levy || max_levy == 0 {
        return error(world, format!(
            "cannot raise on {land_id}: no available levy (wait for the monthly replenishment or dismiss the army in the field)"
        ));
    }

    let army_name = world
        .get::<CharacterOfHouse>(actor_e)
        .and_then(|coh| world.get::<HouseName>(coh.0))
        .map(|hn| format!("{} Army", hn.0))
        .unwrap_or_else(|| "Army".to_string());

    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Army,
            ArmyName(army_name.clone()),
            ArmyLevy(0),
            ArmyMaxLevy(max_levy),
            ArmyOnLand(land_e),
            ArmyBelongsToKingdom(kingdom_e),
            ArmyStatus::Raising,
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    // Flag every ACTIVE building on the land as raised so the monthly `replenishing_levy`
    // loop skips them. The pool value itself is untouched here; only the flag flips.
    let entities: Vec<Entity> = world
        .get::<LandHasBuildings>(land_e)
        .map(|land_has_buildings| land_has_buildings.iter().collect())
        .unwrap_or_default();
    for b_e in entities {
        let active = world
            .get::<BuildingStatus>(b_e)
            .map(|status| *status == BuildingStatus::Active)
            .unwrap_or(false);
        if !active {
            continue;
        }
        if let Some(mut building_is_raised) = world.get_mut::<BuildingIsRaised>(b_e) {
            building_is_raised.0 = true;
        }
    }

    world.trigger(OnArmyRaised { army: eid });
}

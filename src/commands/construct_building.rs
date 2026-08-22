//! The construct-building command: spawn a building instance on a land the
//! actor rules, paid from their treasury.

use super::core::{
    error, land_yield, next_id, picker_row, ruled_lands, set_row_selected, BaseCommand,
    HINT_RED, NAME_COLOR, STAT_COLOR,
};
use crate::app::Game;
use crate::resources::buildings::BuildingDefs;
use crate::ecs::{
    Building, BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand,
    BuildingStatus, CharacterGold, LandHeldBy, Registry, StringId,
};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
use crate::resources::calendar::Calendar;
use crate::observers::{BuildingUpdateKind, OnBuildingUpdated};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

pub struct ConstructBuilding;

impl BaseCommand for ConstructBuilding {
    fn get_command_id(&self) -> &'static str {
        "command:construct_building"
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
                "Construct Building", NAME_COLOR, None, None, None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        let land_pick = choices.iter().find(|(k, _)| k == "land_id").map(|(_, v)| v.clone());
        let building_pick = choices.iter().find(|(k, _)| k == "building_id").map(|(_, v)| v.clone());

        // Step 1: command picked, no land yet → render one row per ruled land.
        if land_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
            let lands = ruled_lands(world, &actor);
            let mut entities = Vec::new();
            for (land_id, land_name) in lands {
                let land_e = world.resource::<Registry>().get(&land_id);
                let (g, l) = land_e.map(|e| land_yield(world, e)).unwrap_or((0, 0));
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("land_id".to_string(), land_id)),
                    &land_name, NAME_COLOR, None,
                    Some((&format_gold(g), STAT_COLOR)),
                    Some((&format_levy(l), STAT_COLOR)),
                );
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 2: land picked, no building yet → render one row per def.
        if building_pick.is_none() {
            let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
            let gold = world
                .resource::<Registry>()
                .get(&actor)
                .and_then(|actor_e| world.get::<CharacterGold>(actor_e))
                .map(|character_gold| character_gold.0)
                .unwrap_or(0);
            let snapshot: Vec<(String, crate::resources::buildings::BuildingDef)> = {
                let defs = world.resource::<BuildingDefs>();
                defs.0.iter().map(|(id, d)| (id.clone(), d.clone())).collect()
            };
            let mut entities = Vec::new();
            for (id, def) in snapshot {
                let cant_afford = (def.construction_price as i64) > gold;
                let name_color = if cant_afford { HINT_RED } else { NAME_COLOR };
                let name = if cant_afford { format!("{} (-cost)", def.name) } else { def.name.clone() };
                let cost_text = format!("{}g", def.construction_price);
                let cost_color = if cant_afford { HINT_RED } else { STAT_COLOR };
                let time_text = world.resource::<Calendar>().format_duration(def.construction_time);
                let row = picker_row(
                    world, parent, self.get_command_id(),
                    Some(("building_id".to_string(), id)),
                    &name, name_color,
                    Some(&building_effect_summary(&def)),
                    Some((&cost_text, cost_color)),
                    Some((&time_text, STAT_COLOR)),
                );
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 3: both picks present → execute.
        let actor = world.resource::<Game>().ctx.player_character_id.clone().unwrap_or_default();
        let land_id = land_pick.as_deref().expect("step 3 reached without a land_id pick");
        let building_id = building_pick.as_deref().expect("step 3 reached without a building_id pick");
        construct(world, &actor, land_id, building_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

/// Render a net-gold-per-month value: `+3g`, `-2g`, or empty for zero.
pub(super) fn format_gold(g: i64) -> String {
    if g > 0 { format!("+{g}g") } else if g < 0 { format!("{g}g") } else { String::new() }
}

/// Render a total-levy value: `20` or empty.
pub(super) fn format_levy(l: u64) -> String {
    if l > 0 { l.to_string() } else { String::new() }
}

/// One-line effect summary for a building def, used as the description on the building picker rows.
pub(super) fn building_effect_summary(def: &crate::resources::buildings::BuildingDef) -> String {
    if def.gold_profit > 0 {
        return format!("+{}g/mo", def.gold_profit);
    }
    if def.gold_upkeep > 0 {
        if def.levy > 0 {
            return format!("-{}g upkeep, +{} levy", def.gold_upkeep, def.levy);
        }
        return format!("-{}g upkeep/mo", def.gold_upkeep);
    }
    if def.levy > 0 {
        if def.fort_level > 0 {
            return format!("+{} levy, fort {}", def.levy, def.fort_level);
        }
        return format!("+{} levy", def.levy);
    }
    if def.fort_level > 0 {
        return format!("fort {}", def.fort_level);
    }
    "no effect".into()
}

/// The validated go-ahead: the entities and numbers `construct` mutates with.
struct Go {
    actor_e: Entity,
    land_e: Entity,
    price: u32,
    def_id: String,
    construction_time: u32,
    def_levy: u32,
}

/// Check the rules against a snapshot: the def exists, the actor rules the land, they can afford the price.
fn validate(world: &World, actor: &str, land_id: &str, def_id: &str) -> Result<Go, String> {
    let registry = world.resource::<Registry>();
    let defs = world.resource::<BuildingDefs>();
    let def = defs.get(def_id).ok_or_else(|| format!("unknown building `{def_id}`"))?;
    let actor_e = registry.get(actor).ok_or_else(|| format!("unknown actor `{actor}`"))?;
    let land_e = registry.get(land_id).ok_or_else(|| format!("no land `{land_id}`"))?;

    let actor_k = get_character_ruled_kingdoms(world, actor_e);
    let land_k = world.get::<LandHeldBy>(land_e).map(|land_held_by| land_held_by.kingdom());
    match (actor_k, land_k) {
        (ks, Some(lk)) if ks.contains(&lk) => {}
        _ => return Err("you don't rule that land".into()),
    }

    let gold = world
        .get::<CharacterGold>(actor_e)
        .map(|character_gold| character_gold.0)
        .unwrap_or(0);
    if gold < def.construction_price as i64 {
        return Err(format!("need {} gold", def.construction_price));
    }

    Ok(Go {
        actor_e,
        land_e,
        price: def.construction_price,
        def_id: def_id.to_string(),
        construction_time: def.construction_time,
        def_levy: def.levy,
    })
}

/// Construct `def_id` on `land_id` for `actor`. Validates, pays, spawns, and logs.
fn construct(world: &mut World, actor: &str, land_id: &str, def_id: &str) {
    let go = match validate(world, actor, land_id, def_id) {
        Ok(g) => g,
        Err(msg) => return error(world, format!("cannot build on {land_id}: {msg}")),
    };

    if let Some(mut character_gold) = world.get_mut::<CharacterGold>(go.actor_e) {
        character_gold.0 -= go.price as i64;
    }

    let finish_date = {
        let calendar = world.resource::<Calendar>();
        let start = *world.resource::<crate::resources::date::Date>();
        start.after_days(go.construction_time, calendar)
    };

    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Building,
            BuildingOf(go.def_id.clone()),
            BuildingOnLand(go.land_e),
            BuildingStatus::Building,
            BuildingConstructionDate(finish_date),
            BuildingLevy(go.def_levy),
            BuildingIsRaised(false),
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);
    world.trigger(OnBuildingUpdated {
        building: eid,
        land: go.land_e,
        kind: BuildingUpdateKind::ConstructionStarted,
    });
}

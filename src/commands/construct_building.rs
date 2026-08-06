//! The construct-building command: spawn a building instance on a land the
//! actor rules, paid from their treasury.
//!
//! All immutable reads happen in [`validate`] (against `&World`); all
//! `&mut World` happens in [`construct`], never tangled. On success it spawns
//! the same bundle [`crate::ecs::populate`] uses, then fires the
//! `OnBuildingUpdated` event so
//! [`on_building_updated`](crate::updates::yields::on_building_updated)
//! re-sums the realm while `LandHasBuildings` is already authoritative.
//!
//! [`recompute_yields`]: crate::updates::yields::recompute_yields

use super::core::{Choice, Command, MenuItem, next_id, note, ruled_lands};
use crate::ecs::{
    Building, BuildingConstructionDate, BuildingOf, BuildingOnLand, BuildingStatus, CharacterGold,
    CharacterLeads, LandHeldBy, LandName, Registry, StringId,
};
use crate::resources::buildings::BuildingDefs;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;

/// Build a building kind on a land the actor rules.
pub struct ConstructBuilding;

impl Command for ConstructBuilding {
    fn name(&self) -> &str {
        "Construct Building"
    }

    fn step_count(&self) -> usize {
        2
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select a land",
            _ => "Select a building",
        }
    }

    fn step_items(
        &self,
        step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        match step {
            // Step 0: the lands the actor rules (can build on).
            0 => ruled_lands(world, actor)
                .into_iter()
                .map(|(id, name)| MenuItem {
                    label: name,
                    value: id,
                })
                .collect(),
            // Step 1: every building kind in the roster — construction is not
            // land-specific. Price + construction time are shown so the player
            // can see both what it costs and how long before it contributes.
            _ => {
                let cal = world.resource::<Calendar>();
                world
                    .resource::<BuildingDefs>()
                    .0
                    .iter()
                    .map(|(id, d)| MenuItem {
                        label: format!(
                            "{} ({}g in {})",
                            d.name,
                            d.construction_price,
                            cal.format_duration(d.construction_time),
                        ),
                        value: id.clone(),
                    })
                    .collect()
            }
        }
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(land_id) = choices.get(0).map(|c| c.value.as_str()) else {
            return;
        };
        let Some(def_id) = choices.get(1).map(|c| c.value.as_str()) else {
            return;
        };
        construct(world, actor, land_id, def_id);
    }
}

/// The validated go-ahead: the entities and numbers [`construct`] mutates with.
struct Go {
    actor_e: Entity,
    land_e: Entity,
    price: u32,
    def_name: String,
    def_id: String,
    construction_time: u32,
    /// Land name captured during the read-only `validate` so the chronicle
    /// line can name the land rather than its bare id.
    land_name: String,
}

/// Check the rules against a snapshot (`&World`): the def exists, the actor
/// rules the land (their kingdom — via [`CharacterLeads`] — equals the land's
/// [`LandHeldBy`]), and they can afford the `construction_price`. Returns the
/// go-ahead or a rejection reason.
fn validate(world: &World, actor: &str, land_id: &str, def_id: &str) -> Result<Go, String> {
    let registry = world.resource::<Registry>();
    let defs = world.resource::<BuildingDefs>();
    let def = defs
        .get(def_id)
        .ok_or_else(|| format!("unknown building `{def_id}`"))?;
    let actor_e = registry
        .get(actor)
        .ok_or_else(|| format!("unknown actor `{actor}`"))?;
    let land_e = registry
        .get(land_id)
        .ok_or_else(|| format!("no land `{land_id}`"))?;

    // Rule check: the actor leads the kingdom that holds the land.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let land_k = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    if actor_k.is_none() || actor_k != land_k {
        return Err("you don't rule that land".into());
    }

    // Afford: no building into debt (boring default; flip to allow debt).
    let gold = world
        .get::<CharacterGold>(actor_e)
        .map(|character_gold| character_gold.0)
        .unwrap_or(0);
    if gold < def.construction_price as i64 {
        return Err(format!("need {} gold", def.construction_price));
    }

    let land_name = world
        .get::<LandName>(land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| land_id.to_string());

    Ok(Go {
        actor_e,
        land_e,
        price: def.construction_price,
        def_name: def.name.clone(),
        def_id: def_id.to_string(),
        construction_time: def.construction_time,
        land_name,
    })
}

/// Construct `def_id` on `land_id` for `actor`. Validates, pays, spawns, and
/// logs. See the module docs for the rules.
fn construct(world: &mut World, actor: &str, land_id: &str, def_id: &str) {
    let go = match validate(world, actor, land_id, def_id) {
        Ok(g) => g,
        Err(msg) => return note(world, format!("cannot build on {land_id}: {msg}")),
    };

    // Pay.
    if let Some(mut character_gold) = world.get_mut::<CharacterGold>(go.actor_e) {
        character_gold.0 -= go.price as i64;
    }

    // Finish date = today + the def's construction time, walked forward
    // under the calendar's month/year lengths so it lands on a valid day
    // even when the construction time crosses year boundaries.
    let (start_date, finish_date) = {
        let calendar = world.resource::<Calendar>();
        let start = *world.resource::<Date>();
        let finish = start.after_days(go.construction_time, calendar);
        (start, finish)
    };

    // Spawn the instance — the relationship hook lands the new building in
    // the land's `LandHasBuildings` synchronously. The building is spawned
    // as `BUILDING` with the calculated finish date; it flips to `ACTIVE`
    // once the day's tick passes that date and yields flow from then on.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Building,
            BuildingOf(go.def_id.clone()),
            BuildingOnLand(go.land_e),
            BuildingStatus::Building,
            BuildingConstructionDate(finish_date),
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);
    // Don't fire `OnBuildingUpdated` here — the new building isn't active
    // yet, so its yield is zero anyway; the `construction` system fires
    // it on the day the building transitions to `ACTIVE`.
    let _ = start_date;

    note(
        world,
        format!(
            "began construction of {} on {} (ready {})",
            go.def_name, go.land_name, finish_date
        ),
    );
}

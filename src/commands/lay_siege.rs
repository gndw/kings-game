//! The siege command: lay siege to a land with one of the player's armies.
//!
//! One selection step: pick an army. The army must be standing on a land
//! that is *not* held by the player's kingdom — the command's `step_items`
//! filters the list to those armies (a siege on your own land is a no-op),
//! so `execute` can trust the data. The picked army's current land is the
//! target; the army's `ArmyStatus` flips to `Sieging` and a fresh
//! [`Siege`](crate::ecs::Siege) entity is spawned with progress 0 and a
//! first event 10 days out. From there the per-day
//! [`tick`](crate::game::siege::tick) advances progress on each scheduled
//! event until 100% — then the siege resolves (buildings flip to
//! `Inactive`, the army gets [`ArmyControlsLand`](crate::ecs::ArmyControlsLand)).
//!
//! The actor must rule the army's kingdom (via `ArmyBelongsToKingdom`) — the
//! same rule every other army command uses.
//!
//! `armies_under` returns the army list in `KingdomHasArmies` order, then
//! `step_items` keeps only the foreign-land entries and reshapes the label
//! to `"<ArmyName> at <LandName>"`.

use super::core::{Choice, Command, MenuItem, next_id, note};
use crate::ecs::{
    ArmyBelongsToKingdom, ArmyName, ArmyOnLand, ArmyStatus, CharacterLeads, KingdomHasArmies,
    LandHeldBy, LandName, Registry, Siege, SiegeAttackerArmy, SiegeDefenderLand,
    SiegeNextEventDate, SiegeProgress, StringId,
};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::world::World;
use bevy::prelude::RelationshipTarget;

/// Lay siege to a land with one of the player's armies. Struct named
/// `LaySiege` to avoid colliding with the [`Siege`](crate::ecs::Siege)
/// marker — the command palette hands a typed `Arc<dyn Command>` around,
/// and the trait's `name()` ("Siege") is what the player sees.
pub struct LaySiege;

impl Command for LaySiege {
    fn name(&self) -> &str {
        "Lay Siege"
    }

    fn step_count(&self) -> usize {
        1
    }

    fn step_title(&self, step: usize) -> &str {
        match step {
            0 => "Select an army",
            _ => "Select an army",
        }
    }

    fn step_items(
        &self,
        _step: usize,
        _choices: &[Choice],
        actor: &str,
        world: &World,
    ) -> Vec<MenuItem> {
        // Mirrors `marching::armies_under` for the army list, then filters
        // to armies currently standing on a foreign land. Filtering in
        // `step_items` (not `execute`) keeps the palette focused — there's
        // no point showing "siege your own capital" as an option.
        let Some(actor_k) = world
            .resource::<Registry>()
            .get(actor)
            .and_then(|actor_e| world.get::<CharacterLeads>(actor_e))
            .map(|character_leads| character_leads.kingdom())
        else {
            return Vec::new();
        };
        let Some(kingdom_has_armies) = world
            .get::<KingdomHasArmies>(actor_k)
            .map(|kha| kha.iter().collect::<Vec<_>>())
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for army_e in kingdom_has_armies {
            let (Some(army_id), Some(army_on_land), Some(army_name)) = (
                world.get::<StringId>(army_e).map(|s| s.0.clone()),
                world.get::<ArmyOnLand>(army_e).map(|a| a.0),
                world.get::<ArmyName>(army_e).map(|n| n.0.clone()),
            ) else {
                continue;
            };
            // Foreign: the land's holding kingdom isn't the actor's. If
            // `LandHeldBy` is missing (defensive) skip the army rather
            // than surface it as a siege option on a broken entity.
            let is_foreign = world
                .get::<LandHeldBy>(army_on_land)
                .map(|land_held_by| land_held_by.kingdom() != actor_k)
                .unwrap_or(false);
            if !is_foreign {
                continue;
            }
            let land_label = world
                .get::<LandName>(army_on_land)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            out.push(MenuItem {
                label: format!("{army_name} at {land_label}"),
                value: army_id,
            });
        }
        out
    }

    fn execute(&self, choices: &[Choice], actor: &str, world: &mut World) {
        let Some(army_id) = choices.first().map(|c| c.value.as_str()) else {
            return;
        };
        begin_siege(world, actor, army_id);
    }
}

/// Spawn the siege entity, flip the army to `Sieging`, schedule the first
/// event 10 days out. Validation re-checks the rules that `step_items`
/// already enforces (army exists, belongs to actor's kingdom, is on a
/// foreign land) — defense in depth in case a future caller bypasses the
/// palette.
fn begin_siege(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot siege with `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot siege with `{army_id}`: no such army"));
    };
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdom());
    let army_k = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    if actor_k.is_none() || actor_k != army_k {
        return note(
            world,
            format!("cannot siege with `{army_id}`: that army does not belong to your kingdom"),
        );
    }
    let Some(land_e) = world
        .get::<ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0)
    else {
        return note(world, format!("cannot siege with `{army_id}`: army has no land"));
    };
    // Foreign check: refuses to siege your own kingdom's lands. Mirrors the
    // step_items filter so the chronicle line is informative even if a
    // player somehow gets here with an army on a friendly land (a stale
    // step_items cache, modded palette, etc.).
    if world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| Some(land_held_by.kingdom()) == actor_k)
        .unwrap_or(true)
    {
        return note(
            world,
            format!("cannot siege with `{army_id}`: that land is your own"),
        );
    }

    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
    let land_name = world
        .get::<LandName>(land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());

    // First event 10 days from today. `Date::after_days` walks the
    // calendar forward so the result lands on a valid month/day even if
    // the +10 crosses a year boundary.
    let next_date = {
        let calendar = world.resource::<Calendar>();
        let today = *world.resource::<Date>();
        today.after_days(10, &calendar)
    };

    // Spawn the siege. `SiegeAttackerArmy` / `SiegeDefenderLand` are
    // Bevy relationships — their hooks fill `ArmyHasSiege` on the army
    // and `LandHasSiegesUnderAttack` on the land synchronously, so any
    // same-frame reader (the very next tick) sees authoritative data.
    let siege_entity_id = next_id(world);
    let siege_e = world
        .spawn((
            StringId(siege_entity_id.clone()),
            Siege,
            SiegeAttackerArmy(army_e),
            SiegeDefenderLand(land_e),
            SiegeProgress(0),
            SiegeNextEventDate(next_date),
        ))
        .id();
    world
        .resource_mut::<Registry>()
        .insert(siege_entity_id, siege_e);

    // Flip the army's status to `Sieging`. The marching tick's match on
    // `Idle` / `Marching` doesn't touch `Sieging` armies — they're locked
    // to the siege until the tick resolves it.
    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Sieging;
    }

    note(
        world,
        format!("{army_name} laid siege to {land_name}"),
    );
}

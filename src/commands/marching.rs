//! The marching-army command: queue marching orders to move one of the
//! actor's armies to another land. A marching is a separate entity
//! (`Marching` + relationships + dates) that the daily marching tick
//! activates when the army is sitting on the marching's source land.
//!
//! **Armies travel by road, one marching per road.** From the land the army
//! stands on, [`march`] traces the road network to the target land (breadth
//! first, so the fewest roads wins) and spawns one marching per road on that
//! route — each carrying `MarchingOnRoad` plus the road's two ends as
//! `MarchingFromLand` / `MarchingToLand`. The chain lands in the army's
//! `ArmyHasMarching` queue in route order and the daily tick walks it hop by
//! hop, each hop costing that road's
//! [`RoadDistanceDays`](crate::ecs::road::RoadDistanceDays). A target with
//! no road route is rejected.
//!
//! Two selection steps: step 0 picks an army (any army under the actor's
//! kingdom), step 1 picks the target land. The actor must rule the army's
//! kingdom (via `ArmyBelongsToKingdom`) and the target land must be a
//! different land from the army's current land.
//!
//! From the actions panel the **G** hotkey opens the palette directly into
//! step 1 with the first army on the selected land pre-picked (mirroring
//! how **B**/**D** open the construct/destroy palette). The "G" row only
//! shows when the player rules the selected land AND at least one player's
//! army sits on it.
//!
//! [`MarchingArmy::execute`] just spawns the marching entities and walks
//! away — the daily tick does the actual lifting (activating the queued
//! marching, moving the army, advancing to the next one in the queue).
//! See [`crate::game::marching::tick`].

use super::core::{
    army_status_text, error, next_id, picker_row, set_row_selected, BaseCommand, NAME_COLOR,
    STAT_COLOR, STAT_DIM,
};
use crate::ecs::army::{
    ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand,
};
use crate::ecs::character::CharacterName;
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::{KingdomHasArmies, KingdomLedBy};
use crate::ecs::marching::{
    Marching, MarchingArmy, MarchingArrivedDate, MarchingBeginDate, MarchingFromLand,
    MarchingOnRoad, MarchingStatus, MarchingToLand,
};
use crate::ecs::road::{Road, RoadBetweenLands};
use crate::ecs::{CharacterLeads, CharacterOfHouse, Land, LandHeldBy, LandName, Registry, StringId};
use crate::events::OnMarchingOrdered;
use crate::ui::command_menu::CommandMenuUiContext;
use crate::app::Game;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::game::marching::road_days;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;
use std::collections::HashMap;
use std::collections::VecDeque;

/// Queue a marching order for one of the actor's armies. Registered as
/// "Marching Army" in the command palette (the player-facing name); the
/// struct is named `MarchingOrder` to avoid colliding with the
/// [`MarchingArmy`](crate::ecs::marching::MarchingArmy) component in
/// `ecs::marching`. One order can spawn several marchings — one per road
/// between the army and the target.
pub struct MarchingOrder;

impl BaseCommand for MarchingOrder {
    fn get_command_id(&self) -> &'static str {
        "command:marching_order"
    }

    fn spawn_command(
        &self,
        world: &mut World,
        parent: Entity,
        choices: &[(String, String)],
    ) -> (Vec<Entity>, bool) {
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        if command_pick.is_none() {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                None,
                "Marching Army",
                NAME_COLOR,
                None,
                None,
                None,
            );
            return (vec![row], false);
        }
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: pick an army.
        let army_pick = choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone());
        if army_pick.is_none() {
            return self.spawn_army_picker(world, parent);
        }

        // Step 2: pick a target land.
        let target_pick = choices
            .iter()
            .find(|(k, _)| k == "target_id")
            .map(|(_, v)| v.clone());
        if target_pick.is_none() {
            return self.spawn_target_picker(world, parent, army_pick.as_deref().unwrap());
        }

        // Execute: both picks present.
        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        set_row_selected(world, entity, is_selected);
    }
}

impl MarchingOrder {
    fn spawn_army_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let armies = armies_under(world, &actor);
        let mut entities = Vec::new();
        for (army_id, name, current_land, levy, status) in armies {
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("army_id".to_string(), army_id)),
                &name,
                NAME_COLOR,
                Some(&format!("at {current_land}")),
                Some((&levy.to_string(), STAT_COLOR)),
                status.as_deref().map(|s| (s, STAT_COLOR)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_target_picker(
        &self,
        world: &mut World,
        parent: Entity,
        army_id: &str,
    ) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let own_kingdoms: std::collections::HashSet<Entity> = world
            .resource::<Registry>()
            .get(&actor)
            .and_then(|actor_e| world.get::<CharacterLeads>(actor_e))
            .map(|character_leads| character_leads.kingdoms().iter().copied().collect())
            .unwrap_or_default();
        let army_land_e = world
            .resource::<Registry>()
            .get(army_id)
            .and_then(|army_e| world.get::<ArmyOnLand>(army_e))
            .map(|aol| aol.0);
        let calendar = world.resource::<Calendar>();
        let rows = all_lands_target_rows(world, army_land_e, &own_kingdoms, calendar);
        let mut entities = Vec::new();
        for row_data in rows {
            // Ruler on the description line (smaller font, fits longer
            // names like "Leyton, Hightower" without clipping); days in
            // stat1 — the field the player most needs to read before
            // committing to a march.
            let row = picker_row(
                world,
                parent,
                self.get_command_id(),
                Some(("target_id".to_string(), row_data.land_id)),
                &row_data.name,
                row_data.name_color,
                row_data.description.as_deref(),
                Some((row_data.days.as_str(), row_data.days_color)),
                None,
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let picks: Vec<(String, String)> = world
            .resource::<CommandMenuUiContext>()
            .choices
            .clone();
        let army_id = picks
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without an army_id pick");
        let target_id = picks
            .iter()
            .find(|(k, _)| k == "target_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a target_id pick");
        march(world, &actor, &army_id, &target_id);
        (Vec::new(), true)
    }
}

/// `(army_id, name, current_land, levy, status_text)` for every army
/// the actor rules. Walks `CharacterLeads` kingdoms and unions their
/// `KingdomHasArmies` lists. `status_text` is `idle` /
/// `→ <land> in <days>d` / `sieging (<progress>%)`. Mirrors
/// `dismiss_army::armies_under` — both commands pick from the actor's
/// army pool, which is the union across every kingdom the actor leads
/// under the multi-kingdom model.
fn armies_under(
    world: &World,
    actor: &str,
) -> Vec<(String, String, String, u64, Option<String>)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let calendar = world.resource::<Calendar>();
    let date = world.resource::<Date>();
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(*kingdom_e) else {
            continue;
        };
        for army_e in kingdom_has_armies.iter() {
            let Some(string_id) = world.get::<StringId>(army_e) else {
                continue;
            };
            let Some(name) = world.get::<ArmyName>(army_e) else {
                continue;
            };
            let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else {
                continue;
            };
            let current_land = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            let status = army_status_text(world, army_e, calendar, date);
            out.push((string_id.0.clone(), name.0.clone(), current_land, levy, status));
        }
    }
    out
}

/// One land's target-picker row data: the land id, the display name
/// (with a suffix when applicable), an optional ruler description, and
/// the days cell. Precomputed by [`all_lands_target_rows`] so the
/// picker spawn loop has no borrows on the world.
struct TargetRowData {
    land_id: String,
    name: String,
    name_color: Color,
    description: Option<String>,
    days: String,
    days_color: Color,
}

/// Walk every land entity and assemble its target-picker row. For each
/// land: read the ruler via `LandHeldBy → KingdomLedBy → CharacterName +
/// CharacterOfHouse → HouseName`, mark `(home)` if the land's kingdom is
/// one of the actor's, and trace the road network from the army's land
/// to compute route days (or `(no route)` if no chain of roads reaches
/// it). `army_land_e` is `None` when the army isn't on a known land —
/// the picker shows every target as unreachable in that case.
fn all_lands_target_rows(
    world: &World,
    army_land_e: Option<Entity>,
    own_kingdoms: &std::collections::HashSet<Entity>,
    calendar: &Calendar,
) -> Vec<TargetRowData> {
    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Land>().is_none() {
            continue;
        }
        let land_e = entity_ref.id();
        let Some(sid) = entity_ref.get::<StringId>() else {
            continue;
        };
        let Some(land_name) = entity_ref.get::<LandName>() else {
            continue;
        };

        // Ruler: walk the kingdom → leader → name + house. Falls back
        // to `?` if any link is missing (torn world or leaderless
        // kingdom in a future release).
        let (ruler_text, _ruler_color) = ruler_text(world, entity_ref.get::<LandHeldBy>());

        // Foreign / own.
        let is_own = entity_ref
            .get::<LandHeldBy>()
            .map(|land_held_by| own_kingdoms.contains(&land_held_by.kingdom()))
            .unwrap_or(false);

        // Route days: trace from the army's current land to this land.
        // `None` means no chain of roads reaches it (unreachable).
        let days_text = match (army_land_e, Some(land_e)) {
            (Some(from), Some(to)) if from != to => match trace(world, from, to) {
                Some(hops) => {
                    let total: u32 = hops
                        .iter()
                        .filter_map(|h| road_days(world, h.road))
                        .sum();
                    calendar.format_duration(total)
                }
                None => "—".into(),
            },
            (Some(from), Some(to)) if from == to => "0d".into(),
            _ => "—".into(),
        };

        // Name with suffix + hint tint.
        let is_home = is_own && army_land_e == Some(land_e);
        let (name, name_color, days_color) = match (is_home, army_land_e, days_text == "—") {
            (true, _, _) => (
                format!("{} (home)", land_name.0),
                super::core::HINT_RED,
                STAT_DIM,
            ),
            (false, Some(from), true) if from != land_e => (
                format!("{} (no route)", land_name.0),
                super::core::HINT_RED,
                STAT_DIM,
            ),
            _ => (land_name.0.clone(), NAME_COLOR, STAT_COLOR),
        };
        let description = if ruler_text.is_empty() {
            None
        } else {
            Some(ruler_text.clone())
        };

        result.push(TargetRowData {
            land_id: sid.0.clone(),
            name,
            name_color,
            description,
            days: days_text,
            days_color,
        });
    }
    result
}

/// One-line ruler label for a kingdom's leader: `"<character>, <house>"`
/// or empty when the kingdom has no leader / the link is missing.
fn ruler_text(world: &World, land_held_by: Option<&LandHeldBy>) -> (String, Color) {
    let Some(kingdom_e) = land_held_by.map(|land_held_by| land_held_by.kingdom()) else {
        return (String::new(), STAT_DIM);
    };
    let Some(kingdom_led_by) = world.get::<KingdomLedBy>(kingdom_e) else {
        return (String::new(), STAT_DIM);
    };
    let leader_e = kingdom_led_by.0;
    let Some(character_name) = world.get::<CharacterName>(leader_e) else {
        return (String::new(), STAT_DIM);
    };
    let house_name = world
        .get::<CharacterOfHouse>(leader_e)
        .and_then(|coh| world.get::<HouseName>(coh.0))
        .map(|hn| hn.0.clone());
    let text = match house_name {
        Some(h) => format!("{}, {}", character_name.0, h),
        None => character_name.0.clone(),
    };
    (text, STAT_COLOR)
}

/// One road on a traced route: the road entity and the land at each end, in
/// travel order. Each hop becomes one marching entity.
struct Hop {
    road: Entity,
    from: Entity,
    to: Entity,
}

/// The road adjacency of the whole map: `land → [(road, other end)]`. Built
/// by walking every `Road` entity's `RoadBetweenLands` (roads are baked at
/// populate time and never change, so this is cheap and always current).
/// Each land's Vec is in road-spawn order, which makes [`trace`]'s search
/// deterministic. Uses `world.iter_entities()` (`&World` safe) rather than
/// `world.query` (which needs `&mut World`).
fn road_graph(world: &World) -> HashMap<Entity, Vec<(Entity, Entity)>> {
    let mut graph: HashMap<Entity, Vec<(Entity, Entity)>> = HashMap::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Road>().is_none() {
            continue;
        }
        let Some(between) = entity_ref.get::<RoadBetweenLands>() else {
            continue;
        };
        // `validate` guarantees exactly two lands; skip anything else
        // rather than index blindly.
        let [a, b] = between.0[..] else { continue };
        let road_e = entity_ref.id();
        graph.entry(a).or_default().push((road_e, b));
        graph.entry(b).or_default().push((road_e, a));
    }
    graph
}

/// Trace the roads from `from_e` to `to_e`, returning one [`Hop`] per road
/// in travel order. Breadth-first, so the route with the fewest roads wins;
/// `None` when no chain of roads connects the two lands (the marching is
/// then rejected — armies never leave the road network). `Some(vec![])` is
/// impossible: `march` rejects `from == to` before calling.
fn trace(world: &World, from_e: Entity, to_e: Entity) -> Option<Vec<Hop>> {
    let graph = road_graph(world);

    // BFS from the army's land, remembering how each land was first
    // reached: `land → (road walked, previous land)`.
    let mut came_from: HashMap<Entity, (Entity, Entity)> = HashMap::new();
    let mut queue: VecDeque<Entity> = VecDeque::from([from_e]);
    while let Some(land_e) = queue.pop_front() {
        if land_e == to_e {
            break;
        }
        for &(road_e, next_e) in graph.get(&land_e).into_iter().flatten() {
            if next_e == from_e || came_from.contains_key(&next_e) {
                continue;
            }
            came_from.insert(next_e, (road_e, land_e));
            queue.push_back(next_e);
        }
    }

    // Walk the predecessors back from the target, then flip into travel
    // order. Bails out if the target was never reached.
    let mut hops = Vec::new();
    let mut cursor = to_e;
    while cursor != from_e {
        let (road_e, prev_e) = *came_from.get(&cursor)?;
        hops.push(Hop { road: road_e, from: prev_e, to: cursor });
        cursor = prev_e;
    }
    hops.reverse();
    Some(hops)
}

/// Validate (actor rules the army; target is a different land; a road route
/// exists), then spawn one marching entity per road on the route — each
/// `MarchingStatus::Scheduled` with empty dates. The daily tick activates
/// them one at a time, each when the army is standing on that hop's source
/// land.
fn march(world: &mut World, actor: &str, army_id: &str, target_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return error(world, format!("cannot march `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return error(world, format!("cannot march `{army_id}`: no such army"));
    };
    let Some(target_e) = world.resource::<Registry>().get(target_id) else {
        return error(world, format!("cannot march to `{target_id}`: no such land"));
    };

    // Rule check: the army's `ArmyBelongsToKingdom` is one of the actor's
    // kingdoms (multi-kingdom: any match counts).
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let _ = match (actor_kingdoms, army_kingdom) {
        (Some(aks), Some(ak)) if aks.contains(&ak) => ak,
        _ => {
            return error(
                world,
                format!(
                    "cannot march `{army_id}`: that army does not belong to your kingdom"
                ),
            );
        }
    };

    // The army's current land is where the route starts. Capture it before
    // we mutate anything so the chronicle line can name it.
    let from_e = world
        .get::<ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0);
    let Some(from_e) = from_e else {
        return error(world, format!("cannot march `{army_id}`: army has no land"));
    };
    if from_e == target_e {
        return error(world, format!(
            "cannot march `{army_id}`: the army is already on {target_id}"
        ));
    }

    let from_name = world
        .get::<LandName>(from_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let to_name = world
        .get::<LandName>(target_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| target_id.to_string());

    // Trace the road network from the army's land to the target. No chain
    // of roads, no march — armies only move along roads.
    let Some(hops) = trace(world, from_e, target_e) else {
        return error(world, format!(
            "cannot march `{army_id}`: no road leads from {from_name} to {to_name}"
        ));
    };

    // Price the route before committing to it: each hop costs its own road's
    // `RoadDistanceDays`, and the sum is what the chronicle quotes, so the
    // player is told exactly what the tick will charge. `None` means a road
    // on the route has no duration — impossible for validated content, and
    // not something to paper over with a guessed number, so the order is
    // refused rather than queued into a march that could never activate.
    let Some(days) = hops
        .iter()
        .map(|hop| road_days(world, hop.road))
        .sum::<Option<u32>>()
    else {
        return error(world, format!(
            "cannot march `{army_id}`: a road on the way to {to_name} has no distance"
        ));
    };

    // One marching entity per road, queued in travel order. The four
    // relationships (`MarchingArmy` / `MarchingFromLand` / `MarchingToLand`
    // / `MarchingOnRoad`) hit Bevy's hooks and auto-fill the reverses on the
    // army, both lands and the road synchronously. Insertion order is what
    // makes `ArmyHasMarching` a route: the tick activates the hop whose
    // source land the army is standing on, which walks the chain in order.
    // Dates stay empty until the daily tick activates each marching.
    for hop in &hops {
        let id = next_id(world);
        let eid = world
            .spawn((
                StringId(id.clone()),
                Marching,
                MarchingArmy(army_e),
                MarchingFromLand(hop.from),
                MarchingToLand(hop.to),
                MarchingOnRoad(hop.road),
                MarchingStatus::Scheduled,
                MarchingBeginDate(None),
                MarchingArrivedDate(None),
            ))
            .id();
        world.resource_mut::<Registry>().insert(id, eid);
    }

    // ponytail: no `ArmyMarching` insertion here — the daily tick does
    // that when activating the first scheduled marching. Until then the
    // army is still Idle (or already Marching on an earlier marching) and
    // the queued hops are just sitting in the army's `ArmyHasMarching`
    // collection waiting for the army to be on the matching source land.

    let roads = hops.len() as u32;

    // Chronicle observer pulls the army / land names off the entities.
    world.trigger(OnMarchingOrdered {
        army: army_e,
        from: from_e,
        to: target_e,
        roads,
        days,
    });
}

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

use super::core::{next_id, note, BaseCommand};
use crate::ecs::army::{
    ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand,
};
use crate::ecs::kingdom::KingdomHasArmies;
use crate::ecs::marching::{
    Marching, MarchingArmy, MarchingArrivedDate, MarchingBeginDate, MarchingFromLand,
    MarchingOnRoad, MarchingStatus, MarchingToLand,
};
use crate::ecs::road::{Road, RoadBetweenLands};
use crate::ecs::{CharacterLeads, LandName, Registry, StringId};
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue, CommandMenuUiContext};
use crate::app::Game;
use crate::game::marching::road_days;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;

/// Queue a marching order for one of the actor's armies. Registered as
/// "Marching Army" in the command palette (the player-facing name); the
/// struct is named `MarchingOrder` to avoid colliding with the
/// [`MarchingArmy`](crate::ecs::marching::MarchingArmy) component in
/// `ecs::marching`. One order can spawn several marchings — one per road
/// between the army and the target.
pub struct MarchingOrder;

// --- palette UI -------------------------------------------------------------
// Same shape as the other commands: a single padded card whose title text
// is the command's display name. The shared `update` swaps the background
// between `ROW_PANEL` and `ROW_PANEL_SELECTED`.

/// Per-row background in the palette.
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);

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
            return self.spawn_command_row(world, parent);
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
            return self.spawn_target_picker(world, parent);
        }

        // Execute: both picks present.
        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

impl MarchingOrder {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = self
            .spawn_row(
                world,
                parent,
                "Marching Army",
                None,
            );
        (vec![row], false)
    }

    fn spawn_army_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let armies = armies_under(world, &actor);
        let mut entities = Vec::new();
        for (army_id, label) in armies {
            let row = self.spawn_row(
                world,
                parent,
                &label,
                Some(("army_id".to_string(), army_id)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_target_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let lands = all_lands(world);
        let mut entities = Vec::new();
        for (land_id, name) in lands {
            let row = self.spawn_row(
                world,
                parent,
                &name,
                Some(("target_id".to_string(), land_id)),
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

    /// Helper: spawn a single styled row with optional `CommandHasKey` /
    /// `CommandHasValue` attached. Used by the command / army / target
    /// pickers above.
    fn spawn_row(
        &self,
        world: &mut World,
        parent: Entity,
        title: &str,
        key_value: Option<(String, String)>,
    ) -> Entity {
        let mut entity = world.spawn((
            Node {
                width: percent(100),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(ROW_PANEL),
            BorderColor::all(ROW_BORDER),
            ChildOf(parent),
            CommandHasId(self.get_command_id().to_string()),
        ));
        if let Some((k, v)) = key_value {
            entity.insert((
                CommandHasKey(k),
                CommandHasValue(v),
            ));
        }
        let row = entity.id();
        world.entity_mut(row).with_children(|c| {
            c.spawn((
                Text::new(title),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
        row
    }
}

/// `(army_id, "<land>: <levy>")` for every army in every kingdom the actor
/// leads, in `CharacterLeads` order followed by `KingdomHasArmies`. Mirrors
/// `dismiss_army::armies_under` — both commands pick from the actor's army
/// pool, which is the union across every kingdom the actor leads under
/// the multi-kingdom model.
fn armies_under(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kingdom_has_armies) = world.get::<KingdomHasArmies>(*kingdom_e) else {
            continue;
        };
        for army_e in kingdom_has_armies.iter() {
            let army_id = match world.get::<StringId>(army_e) {
                Some(s) => s.0.clone(),
                None => continue,
            };
            let army_on_land = match world.get::<ArmyOnLand>(army_e) {
                Some(a) => a,
                None => continue,
            };
            let land_name = world
                .get::<LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            out.push((army_id, format!("{land_name}: {levy}")));
        }
    }
    out
}

/// `(land_id, land_name)` for every land in the world. The player picks
/// the target; `execute` rejects the same land as the army's current land.
/// Filters by the `Land` marker so we only see land entities (every land
/// carries `LandName`; the marker is the canonical land discriminant).
/// Uses `world.iter_entities()` (`&World` safe) rather than `world.query`
/// (which needs `&mut World`).
fn all_lands(world: &World) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<crate::ecs::Land>().is_none() {
            continue;
        }
        let (Some(sid), Some(name)) = (
            entity_ref.get::<StringId>(),
            entity_ref.get::<LandName>(),
        ) else {
            continue;
        };
        result.push((sid.0.clone(), name.0.clone()));
    }
    result
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
        return note(world, format!("cannot march `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot march `{army_id}`: no such army"));
    };
    let Some(target_e) = world.resource::<Registry>().get(target_id) else {
        return note(world, format!("cannot march to `{target_id}`: no such land"));
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
            return note(
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
        return note(world, format!("cannot march `{army_id}`: army has no land"));
    };
    if from_e == target_e {
        return note(world, format!(
            "cannot march `{army_id}`: the army is already on {target_id}"
        ));
    }

    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
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
        return note(world, format!(
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
        return note(world, format!(
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
    let plural = if roads == 1 { "road" } else { "roads" };
    note(
        world,
        format!("queued {army_name} march: {from_name} → {to_name} ({roads} {plural}, {days} days)"),
    );
}

//! The construct-building command: spawn a building instance on a land the
//! actor rules, paid from their treasury.
//!
//! All immutable reads happen in [`validate`] (against `&World`); all
//! `&mut World` happens in [`construct`], never tangled. On success it spawns
//! the same bundle [`crate::ecs::populate`] uses, then fires the
//! `OnBuildingUpdated` event so
//! [`on_building_updated`](crate::game::yields::on_building_updated)
//! re-sums the realm while `LandHasBuildings` is already authoritative.
//!
//! [`recompute_yields`]: crate::game::yields::recompute_yields

use super::core::{next_id, note, ruled_lands, BaseCommand};
use crate::app::Game;
use crate::resources::buildings::BuildingDefs;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue};
use crate::ecs::{
    Building, BuildingConstructionDate, BuildingIsRaised, BuildingLevy, BuildingOf, BuildingOnLand,
    BuildingStatus, CharacterGold, CharacterLeads, LandHeldBy, LandName, Registry, StringId,
};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

/// Build a building kind on a land the actor rules.
pub struct ConstructBuilding;

// --- palette UI -------------------------------------------------------------
// For now: a single padded card with the command's title text.

/// Per-row background in the palette. One shade lighter than the panel.
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);

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
        // The current pick (if any) — `(key, value)` where key is
        // `"command"`. Each branch bails out early so the happy path
        // stays at the bottom of the function.
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        // No `"command"` key → first open, render the command row as
        // usual. Sits above the land render so the source follows the
        // player's selection order: pick a command first, then a land.
        if command_pick.is_none() {
            let row = world
                .spawn((
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
                ))
                .id();
            world.entity_mut(row).with_children(|c| {
                c.spawn((
                    Text::new("Construct Building"),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::srgb(0.96, 0.96, 0.98)),
                ));
            });
            return (vec![row], false);
        }

        // `"command"` key, value mismatch → another command was picked,
        // skip this row entirely.
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Pre-step: pull the running step picks out of `choices` so the
        // rest of the function can branch on them.
        let land_pick = choices
            .iter()
            .find(|(k, _)| k == "land_id")
            .map(|(_, v)| v.clone());
        let building_pick = choices
            .iter()
            .find(|(k, _)| k == "building_id")
            .map(|(_, v)| v.clone());

        // Step 1: command picked, no land yet → render one row per
        // land the player currently rules.
        if land_pick.is_none() {
            let actor = world
                .resource::<Game>()
                .ctx
                .player_character_id
                .clone();
            let lands = ruled_lands(world, &actor);
            let mut entities = Vec::new();
            for (land_id, land_name) in lands {
                let row = world
                    .spawn((
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
                        CommandHasKey("land_id".to_string()),
                        CommandHasValue(land_id),
                    ))
                    .id();
                world.entity_mut(row).with_children(|c| {
                    c.spawn((
                        Text::new(land_name),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::srgb(0.96, 0.96, 0.98)),
                    ));
                });
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 2: land picked, no building yet → render one row per
        // building kind from `BuildingDefs`. Snapshot the defs first so
        // the immutable `Resource` borrow drops before we spawn.
        if building_pick.is_none() {
            let snapshot: Vec<(String, String)> = {
                let defs = world.resource::<BuildingDefs>();
                defs.0
                    .iter()
                    .map(|(id, def)| (id.clone(), def.name.clone()))
                    .collect()
            };
            let mut entities = Vec::new();
            for (id, name) in snapshot {
                let row = world
                    .spawn((
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
                        CommandHasKey("building_id".to_string()),
                        CommandHasValue(id),
                    ))
                    .id();
                world.entity_mut(row).with_children(|c| {
                    c.spawn((
                        Text::new(name),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::srgb(0.96, 0.96, 0.98)),
                    ));
                });
                entities.push(row);
            }
            return (entities, false);
        }

        // Step 3: both picks present → execute. `construct` validates the
        // picks against the world (actor must rule the land, must be
        // able to afford the def) and pays + spawns the building on
        // success. Failures land in the chronicle via `note` inside
        // `construct`; the panel still closes either way.
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let land_id = land_pick
            .as_deref()
            .expect("step 3 reached without a land_id pick");
        let building_id = building_pick
            .as_deref()
            .expect("step 3 reached without a building_id pick");
        construct(world, &actor, land_id, building_id);
        (Vec::new(), true)
    }
    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        // The row is the one we spawned; the orchestrator's
        // `crate::commands::core::update` passes it in. Swap the background
        // to the highlight colour when the cursor lands on this row.
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
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
    /// Def's `levy` — used at spawn time to seed `BuildingLevy` with the
    /// full pool. Captured here so `construct` doesn't need to re-look-up
    /// the def.
    def_levy: u32,
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

    // Rule check: any of the actor's kingdoms holds the land.
    let actor_k = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let land_k = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    match (actor_k, land_k) {
        (Some(ks), Some(lk)) if ks.contains(&lk) => {}
        _ => return Err("you don't rule that land".into()),
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
        def_levy: def.levy,
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
            // Spawn-time seed: full pool, not raised. `BuildingIsRaised` stays
            // `false` until `RaiseArmy` actually drains this building.
            BuildingLevy(go.def_levy),
            BuildingIsRaised(false),
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

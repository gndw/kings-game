//! The raise-army command: spawn an [`Army`](crate::ecs::army::Army) entity on a
//! land the actor rules.
//!
//! One selection step (pick a ruled land); the action then immediately spawns
//! the army on enter. Reach it through the command palette (**C** then pick
//! *Raise Army*).
//!
//! Initial levy comes from the per-building `BuildingLevy` pool (not from
//! the defs directly): the sum of every ACTIVE building's available levy
//! on the land. The raise then *drains* those pools to `0` and flags the
//! buildings with `BuildingIsRaised = true` so the second raise on the
//! same land is rejected. The monthly
//! [`replenish_levy`](crate::game::replenish_levy::replenish) loop fills
//! the pools back up over time.

use super::core::{available_levy, drain_buildings, next_id, note, ruled_lands, BaseCommand};
use crate::app::Game;
use crate::ecs::army::{Army, ArmyBelongsToKingdom, ArmyLevy, ArmyName, ArmyOnLand, ArmyStatus};
use crate::ecs::{
    CharacterLeads, CharacterOfHouse, HouseName, LandHeldBy, LandName, Registry, StringId,
};
use crate::events::{BuildingUpdateKind, OnArmyRaised, OnBuildingUpdated};
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

// --- palette UI -------------------------------------------------------------
// Same shape as `construct_building`: a single padded card whose title
// text is the command's display name. The shared `update` swaps the
// background between `ROW_PANEL` and `ROW_PANEL_SELECTED`.

/// Per-row background in the palette. One shade lighter than the panel.
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);

/// Raise an army on a land the actor rules.
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
        let command_pick = choices
            .iter()
            .find(|(k, _)| k == "command")
            .map(|(_, v)| v.as_str());

        // No `"command"` key → render the command row.
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
                    Text::new("Raise Army"),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::srgb(0.96, 0.96, 0.98)),
                ));
            });
            return (vec![row], false);
        }

        // Mismatch → skip.
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: render one row per land the player rules.
        let land_pick = choices
            .iter()
            .find(|(k, _)| k == "land_id")
            .map(|(_, v)| v.clone());
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

        // Execute: both picks present → call the existing function.
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let land_id = land_pick
            .as_deref()
            .expect("step 1 reached without a land_id pick");
        raise(world, &actor, land_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

/// Spawn the army. Validates the actor rules the land, sums the available
/// `BuildingLevy` pools (refusing if none), drains them, creates the army
/// bundle, registers the id, and appends a chronicle line.
fn raise(world: &mut World, actor: &str, land_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot raise on {land_id}: unknown actor"));
    };
    let Some(land_e) = world.resource::<Registry>().get(land_id) else {
        return note(world, format!("cannot raise on {land_id}: no such land"));
    };

    // Rule check: any of the actor's kingdoms holds the land. Multi-kingdom:
    // the army's kingdom is the specific kingdom that holds the chosen land
    // (so `ArmyBelongsToKingdom` is the holding kingdom, not "the player's
    // kingdom" generically — the player can have several).
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let land_kingdom = world
        .get::<LandHeldBy>(land_e)
        .map(|land_held_by| land_held_by.kingdom());
    let kingdom_e = match (actor_kingdoms, land_kingdom) {
        (Some(ks), Some(lk)) if ks.contains(&lk) => lk,
        _ => {
            return note(
                world,
                format!("cannot raise on {land_id}: you don't rule that land"),
            );
        }
    };

    let land_name = world
        .get::<LandName>(land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| land_id.to_string());

    // Pool gate: refuse when there's no `BuildingLevy` to draw from.
    let (initial_levy, has_levy) = available_levy(world, land_e);
    if !has_levy || initial_levy == 0 {
        return note(world, format!(
            "cannot raise on {land_id}: no available levy (wait for the monthly replenishment or dismiss the army in the field)"
        ));
    }

    // Default army name: `<house> Army`.
    let army_name = world
        .get::<CharacterOfHouse>(actor_e)
        
            .and_then(|coh| world.get::<HouseName>(coh.0))
            .map(|hn| format!("{} Army", hn.0))
            .unwrap_or_else(|| "Army".to_string());

    // Spawn the army bundle.
    let id = next_id(world);
    let eid = world
        .spawn((
            StringId(id.clone()),
            Army,
            ArmyName(army_name.clone()),
            ArmyLevy(initial_levy),
            ArmyOnLand(land_e),
            ArmyBelongsToKingdom(kingdom_e),
            ArmyStatus::Idle,
        ))
        .id();
    world.resource_mut::<Registry>().insert(id, eid);

    let drained = drain_buildings(world, land_e);

    note(
        world,
        format!("raised {army_name} on {land_name} ({initial_levy} levy)"),
    );

    world.trigger(OnArmyRaised { army: eid });
    for b_e in drained {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: land_e,
            kind: BuildingUpdateKind::Raised,
        });
    }
}

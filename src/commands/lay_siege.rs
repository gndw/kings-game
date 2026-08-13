//! The siege command: lay siege to a land with one of the player's armies.
//!
//! One selection step: pick an army. The army must be standing on a land
//! that is *not* held by the player's kingdom. The picked army's current
//! land is the target; the army's `ArmyStatus` flips to `Sieging` and a
//! fresh [`Siege`](crate::ecs::Siege) entity is spawned with progress 0
//! and a first event 10 days out. From there the per-day
//! [`tick`](crate::game::siege::tick) advances progress on each scheduled
//! event until 100% — then the siege resolves (buildings flip to
//! `Inactive`, the army gets [`ArmyControlsLand`](crate::ecs::ArmyControlsLand)).
//!
//! The actor must rule the army's kingdom (via `ArmyBelongsToKingdom`) — the
//! same rule every other army command uses.

use super::core::{note, BaseCommand};
use crate::app::Game;
use crate::ecs::{
    ArmyBelongsToKingdom, ArmyName, ArmyOnLand, ArmyStatus, CharacterLeads, KingdomHasArmies,
    LandHeldBy, LandName, Registry, Siege, SiegeAttackerArmy, SiegeDefenderLand,
    SiegeNextEventDate, SiegeProgress, StringId,
};
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue, CommandMenuUiContext};
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::prelude::*;

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

/// Lay siege to a land with one of the player's armies.
pub struct LaySiege;

impl BaseCommand for LaySiege {
    fn get_command_id(&self) -> &'static str {
        "command:lay_siege"
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

        // Step 1: pick the army.
        let army_pick = choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone());
        match army_pick {
            None => self.spawn_army_picker(world, parent),
            Some(_) => self.execute(world),
        }
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

impl LaySiege {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
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
                Text::new("Lay Siege"),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
        (vec![row], false)
    }

    fn spawn_army_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let armies = foreign_armies_under(world, &actor);
        let mut entities = Vec::new();
        for (army_id, label) in armies {
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
                    CommandHasKey("army_id".to_string()),
                    CommandHasValue(army_id),
                ))
                .id();
            world.entity_mut(row).with_children(|c| {
                c.spawn((
                    Text::new(label),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::srgb(0.96, 0.96, 0.98)),
                ));
            });
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let army_id = world
            .resource::<CommandMenuUiContext>()
            .choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without an army_id pick");
        begin_siege(world, &actor, &army_id);
        (Vec::new(), true)
    }
}

/// `(army_instance_id, "<ArmyName> at <LandName>")` for every army under
/// the actor's kingdoms that's currently standing on a *foreign* land.
fn foreign_armies_under(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let actor_kingdoms: std::collections::HashSet<Entity> =
        character_leads.kingdoms().iter().copied().collect();
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(kha) = world.get::<KingdomHasArmies>(*kingdom_e) else {
            continue;
        };
        for army_e in kha.iter() {
            let (Some(army_id), Some(aol), Some(army_name)) = (
                world.get::<StringId>(army_e).map(|s| s.0.clone()),
                world.get::<ArmyOnLand>(army_e).map(|a| a.0),
                world.get::<ArmyName>(army_e).map(|n| n.0.clone()),
            ) else {
                continue;
            };
            let is_foreign = world
                .get::<LandHeldBy>(aol)
                .map(|lhb| !actor_kingdoms.contains(&lhb.kingdom()))
                .unwrap_or(false);
            if !is_foreign {
                continue;
            }
            let land_label = world
                .get::<LandName>(aol)
                .map(|ln| ln.0.clone())
                .unwrap_or_else(|| "?".into());
            out.push((army_id, format!("{army_name} at {land_label}")));
        }
    }
    out
}

/// Spawn the siege entity, flip the army to `Sieging`, schedule the first
/// event 10 days out.
/// Spawn the siege entity, flip the army to `Sieging`, schedule the first
/// event 10 days out.
fn begin_siege(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot siege with `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot siege with `{army_id}`: no such army"));
    };

    // Snapshot the data we need (actor kingdoms, the army's land, the land's
    // holding kingdom) so the immutable borrows drop before we mutate
    // `world` to flip the army's status and spawn the siege entity.
    let (actor_kingdoms, army_land_e, is_foreign) = {
        let actor_kingdoms: std::collections::HashSet<Entity> = world
            .get::<CharacterLeads>(actor_e)
            .map(|cl| cl.kingdoms().iter().copied().collect())
            .unwrap_or_default();
        let _army_kingdom = world
            .get::<ArmyBelongsToKingdom>(army_e)
            .map(|abtk| abtk.0);
        let Some(army_on_land) = world.get::<ArmyOnLand>(army_e) else {
            return;
        };
        let is_foreign = world
            .get::<LandHeldBy>(army_on_land.0)
            .map(|lhb| !actor_kingdoms.contains(&lhb.kingdom()))
            .unwrap_or(false);
        (actor_kingdoms, army_on_land.0, is_foreign)
    };

    if !world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|abtk| actor_kingdoms.contains(&abtk.0))
        .unwrap_or(false)
    {
        return note(
            world,
            format!("cannot siege with `{army_id}`: that army does not belong to your kingdom"),
        );
    }
    if !is_foreign {
        return note(
            world,
            format!("cannot siege with `{army_id}`: a siege on your own land is a no-op"),
        );
    }

    if let Some(mut army_status) = world.get_mut::<ArmyStatus>(army_e) {
        *army_status = ArmyStatus::Sieging;
    }
    let today = *world.resource::<Date>();
    let next_event = {
        let calendar = world.resource::<Calendar>();
        today.after_days(10, calendar)
    };
    let _siege_e = world
        .spawn((
            Siege,
            SiegeAttackerArmy(army_e),
            SiegeDefenderLand(army_land_e),
            SiegeProgress(0),
            SiegeNextEventDate(next_event),
        ))
        .id();

    let land_label = world
        .get::<LandName>(army_land_e)
        .map(|ln| ln.0.clone())
        .unwrap_or_else(|| "?".into());
    note(
        world,
        format!("laid siege with `{army_id}` on {land_label} (first event {next_event})"),
    );
}


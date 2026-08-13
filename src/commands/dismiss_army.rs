//! The dismiss-army command: despawn an [`Army`](crate::ecs::army::Army) the actor
//! rules. The inverse of [`super::raise_army`].
//!
//! One selection step listing every army under the actor's kingdom (across all
//! lands, since the kingdom currently owns just one but the listing mirrors
//! the data shape — the army list walks `KingdomHasArmies`). The pick
//! despawns the entity, which Bevy's relationship hooks use to pull it out of
//! the land's `LandHasArmies` and the kingdom's `KingdomHasArmies`; we then
//! deregister the runtime id. The army's levy is distributed back into the
//! kingdom's home land's `BuildingLevy` pools (the ones *raised* drained on
//! the way up) — regardless of which land the army currently sits on. So a
//! dismissed army that marched away still returns its levy home.

use super::core::{distribute_levy_back, note, BaseCommand};
use crate::app::Game;
use crate::ecs::army::{ArmyBelongsToKingdom, ArmyHasMarching, ArmyLevy, ArmyName};
use crate::ecs::kingdom::KingdomHold;
use crate::ecs::{CharacterLeads, KingdomHasArmies, Registry, StringId};
use crate::events::{BuildingUpdateKind, OnArmyDismiss, OnBuildingUpdated};
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

/// Dismiss one of the armies the actor rules.
pub struct DismissArmy;

impl BaseCommand for DismissArmy {
    fn get_command_id(&self) -> &'static str {
        "command:dismiss_army"
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
        // usual.
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
                    Text::new("Dismiss Army"),
                    TextFont::from_font_size(16.0),
                    TextColor(Color::srgb(0.96, 0.96, 0.98)),
                ));
            });
            return (vec![row], false);
        }

        // `"command"` key, value mismatch → another command was picked.
        if command_pick != Some(self.get_command_id()) {
            return (Vec::new(), false);
        }

        // Step 1: render one row per army the player rules.
        let army_pick = choices
            .iter()
            .find(|(k, _)| k == "army_id")
            .map(|(_, v)| v.clone());
        if army_pick.is_none() {
            let actor = world
                .resource::<Game>()
                .ctx
                .player_character_id
                .clone();
            let armies = armies_under(world, &actor);
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
            return (entities, false);
        }

        // Execute: both picks present → call the existing function.
        let actor = world
            .resource::<Game>()
            .ctx
            .player_character_id
            .clone();
        let army_id = army_pick
            .as_deref()
            .expect("step 1 reached without an army_id pick");
        dismiss(world, &actor, army_id);
        (Vec::new(), true)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

/// `(army_instance_id, "<land>:<levy>")` for every army in every kingdom
/// the actor leads, in `CharacterLeads` order followed by `KingdomHasArmies`.
/// Multi-kingdom: the player can rule several kingdoms at once, so the army
/// list is the union across every kingdom they lead. Walks the relationship
/// targets via `world::get` so it stays a `&World` read.
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
            let string_id = match world.get::<StringId>(army_e) {
                Some(s) => s.0.clone(),
                None => continue,
            };
            let army_on_land = match world.get::<crate::ecs::army::ArmyOnLand>(army_e) {
                Some(a) => a,
                None => continue,
            };
            let land_name = world
                .get::<crate::ecs::LandName>(army_on_land.0)
                .map(|land_name| land_name.0.clone())
                .unwrap_or_else(|| "?".into());
            let levy = world
                .get::<ArmyLevy>(army_e)
                .map(|army_levy| army_levy.0)
                .unwrap_or(0);
            out.push((string_id, format!("{land_name}: {levy}")));
        }
    }
    out
}

/// Despawn the army `army_id` for `actor
/// Validates the actor's kingdom owns
/// the army, then despawns + deregisters. Despawning auto-pulls the army out
/// of both `LandHasArmies` and `KingdomHasArmies` via Bevy's relationship
/// hooks. Any queued marchings under the army are reaped first so the
/// marchings don't outlive their `MarchingArmy` target.
fn dismiss(world: &mut World, actor: &str, army_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, format!("cannot dismiss `{army_id}`: unknown actor"));
    };
    let Some(army_e) = world.resource::<Registry>().get(army_id) else {
        return note(world, format!("cannot dismiss `{army_id}`: no such army"));
    };
    // Rule check: the army's `ArmyBelongsToKingdom` is one of the actor's
    // kingdoms (multi-kingdom: any of them counts).
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>());
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(army_e)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let kingdom_e = match (actor_kingdoms, army_kingdom) {
        (Some(aks), Some(ak)) if aks.contains(&ak) => ak,
        _ => {
            return note(
                world,
                format!(
                    "cannot dismiss `{army_id}`: that army does not belong to your kingdom"
                ),
            );
        }
    };

    // Two lands to distinguish:
    // - `army_land_e`: the land the army is currently sitting on (for the
    //   chronicle line). The army may have marched away from home.
    // - `kingdom_land_e`: the kingdom's home land — the one whose
    //   `BuildingLevy` pools the army drained on raise, and the one they
    //   fill back into on dismiss. The levy always returns home, not to
    //   whatever land the army happens to be on at dismiss time.
    let army_land_e = world
        .get::<crate::ecs::army::ArmyOnLand>(army_e)
        .map(|army_on_land| army_on_land.0);
    let army_land_name = army_land_e
        .and_then(|e| world.get::<crate::ecs::LandName>(e))
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let kingdom_land_e = world
        .get::<KingdomHold>(kingdom_e)
        .map(|kingdom_hold| kingdom_hold.0);
    let Some(kingdom_land_e) = kingdom_land_e else {
        return note(world, format!("cannot dismiss `{army_id}`: kingdom has no land"));
    };
    let kingdom_land_name = world
        .get::<crate::ecs::LandName>(kingdom_land_e)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());

    let army_name = world
        .get::<ArmyName>(army_e)
        .map(|army_name| army_name.0.clone())
        .unwrap_or_else(|| "Army".to_string());
    let army_levy = world
        .get::<ArmyLevy>(army_e)
        .map(|army_levy| army_levy.0)
        .unwrap_or(0);

    // Distribute the army's levy back into the kingdom's-land buildings
    // BEFORE the despawn.
    let dismissed = distribute_levy_back(world, kingdom_land_e, army_levy);

    // Reap queued marchings first.
    let queued: Vec<bevy::ecs::entity::Entity> = world
        .get::<ArmyHasMarching>(army_e)
        .map(|q| q.iter().collect())
        .unwrap_or_default();
    for m_e in queued {
        world.despawn(m_e);
    }

    // Despawn + deregister.
    world.entity_mut(army_e).despawn();
    world.resource_mut::<Registry>().by_id.remove(army_id);

    note(
        world,
        format!(
            "dismissed {army_name} on {army_land_name} ({army_levy} levy returned to {kingdom_land_name})"
        ),
    );

    world.trigger(OnArmyDismiss { army: army_e });
    for b_e in dismissed {
        world.trigger(OnBuildingUpdated {
            building: b_e,
            land: kingdom_land_e,
            kind: BuildingUpdateKind::Dismissed,
        });
    }
}

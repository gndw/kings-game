//! The declare-war command: declare war on another kingdom for a casus belli.
//!
//! Two selection steps: step 0 picks a defender kingdom (any kingdom other
//! than the actor's own), step 1 picks a casus belli type (only `Conquest`
//! exists today). The pick spawns a [`War`](crate::ecs::War) entity
//! linking the actor's kingdom (attacker) to the defender with a
//! [`WarCasusBelliType`] and a [`WarDemands`] list — for `Conquest`, the
//! list is auto-seeded with one [`WarDemandType::Take`] on the defender
//! kingdom, which the [`EnforceDemands`] command can resolve.
//!
//! The war has no status / no tick / no resolution yet — the entity
//! exists so the relationship graph is wired (kingdom → wars →
//! demands → target kingdom) and the chronicle records the declaration.
//! Resolution lands in [`EnforceDemands`].
//!
//! The actor's kingdom is read through [`CharacterLeads`]; the kingdom
//! has no name field of its own, so the kingdom's display label is the
//! name of its held land (the convention everywhere else in the codebase
//! — a kingdom's seat is its single land).

use super::core::{next_id, note, BaseCommand};
use crate::ecs::{
    CharacterLeads, Kingdom, KingdomHold, LandName, Registry, StringId, War, WarAttackerKingdom,
    WarBeginDate, WarCasusBelliType, WarDefenderKingdom, WarDemand, WarDemandType, WarDemands,
    WarName,
};
use crate::app::Game;
use crate::resources::date::Date;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue, CommandMenuUiContext};
use bevy::ecs::world::World;
use bevy::prelude::*;

/// Declare war on a kingdom under a casus belli.

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

impl BaseCommand for DeclareWar {
    fn get_command_id(&self) -> &'static str {
        "command:declare_war"
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

        // Step 1: pick defender kingdom.
        let defender_pick = choices
            .iter()
            .find(|(k, _)| k == "defender_id")
            .map(|(_, v)| v.clone());
        if defender_pick.is_none() {
            return self.spawn_defender_picker(world, parent);
        }

        // Step 2: pick casus belli.
        let cb_pick = choices
            .iter()
            .find(|(k, _)| k == "cb_id")
            .map(|(_, v)| v.clone());
        if cb_pick.is_none() {
            return self.spawn_cb_picker(world, parent);
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

impl DeclareWar {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = self.spawn_row(world, parent, "Declare War", None);
        (vec![row], false)
    }

    fn spawn_defender_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        // Snapshot the picker result so the immutable borrows drop.
        let others = other_kingdoms(world, &actor);
        let mut entities = Vec::new();
        for (def_id, label) in others {
            let row = self.spawn_row(
                world,
                parent,
                &label,
                Some(("defender_id".to_string(), def_id)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_cb_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        // Only one CB exists today.
        let row = self.spawn_row(
            world,
            parent,
            "Conquest (seize their land)",
            Some((
                "cb_id".to_string(),
                "conquest".to_string(),
            )),
        );
        (vec![row], false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let picks: Vec<(String, String)> =
            world.resource::<CommandMenuUiContext>().choices.clone();
        let defender_id = picks
            .iter()
            .find(|(k, _)| k == "defender_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a defender_id pick");
        let cb_id = picks
            .iter()
            .find(|(k, _)| k == "cb_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a cb_id pick");
        declare(world, &actor, &defender_id, &cb_id);
        (Vec::new(), true)
    }

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
            entity.insert((CommandHasKey(k), CommandHasValue(v)));
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
pub struct DeclareWar;

/// `(kingdom_id, "<land_name>")` for every kingdom in the world except
/// the actor's. Multi-kingdom: any of the actor's kingdoms counts as
/// "own", so the filter excludes every entry in `CharacterLeads`.
/// Walks `World::iter_entities` (the `&World`-safe path — `query` needs
/// `&mut World`); filters by the [`Kingdom`] marker so we only see
/// kingdom entities.
fn other_kingdoms(world: &World, actor: &str) -> Vec<(String, String)> {
    let own_kingdoms: std::collections::HashSet<bevy::ecs::entity::Entity> = world
        .resource::<Registry>()
        .get(actor)
        .and_then(|actor_e| world.get::<CharacterLeads>(actor_e))
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect())
        .unwrap_or_default();

    let mut result = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<Kingdom>().is_none() {
            continue;
        }
        let kingdom_e = entity_ref.id();
        if own_kingdoms.contains(&kingdom_e) {
            continue;
        }
        let Some(string_id) = entity_ref.get::<StringId>() else {
            continue;
        };
        // The kingdom's display label is the name of its held land — a
        // kingdom has no name field of its own (its seat is its land).
        let label = entity_ref
            .get::<KingdomHold>()
            .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
            .map(|land_name| land_name.0.clone())
            .unwrap_or_else(|| string_id.0.clone());
        result.push((string_id.0.clone(), label));
    }
    result
}

/// Resolve the picked CB id to its [`WarCasusBelliType`]. Only `Conquest`
/// exists today; unknown ids are rejected. New CB enum variants are added
/// here (the menu row in `step_items` is the only other place).
fn resolve_cb(cb_id: &str) -> Option<WarCasusBelliType> {
    match cb_id {
        "conquest" => Some(WarCasusBelliType::Conquest),
        _ => None,
    }
}

/// Seed the war's initial demands from the picked CB type + the defender
/// kingdom. `Conquest` adds one `Take(defender_kingdom)` demand — the
/// archetype for a conquest war is "make this kingdom ours". New CB
/// shapes are additive: a `Reparations` arm would seed a different
/// demand mix (or none, depending on the shape).
fn demands_for(cb_type: WarCasusBelliType, defender_kingdom_e: bevy::ecs::entity::Entity) -> Vec<WarDemand> {
    match cb_type {
        WarCasusBelliType::Conquest => vec![WarDemand {
            demand_type: WarDemandType::Take,
            target: defender_kingdom_e,
        }],
    }
}

/// Validate (actor rules a kingdom; defender is a different kingdom; CB id
/// resolves), then spawn a [`War`] entity linking the actor's kingdom to
/// the defender with the picked CB type and an auto-seeded demand list.
/// Appends a chronicle line on success and on every rejection.
fn declare(world: &mut World, actor: &str, defender_id: &str, cb_id: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, "cannot declare war: unknown actor".into());
    };
    // Multi-kingdom: pick the first kingdom the actor leads as the
    // `WarAttackerKingdom`. A future "pick which kingdom declares war"
    // step would let the player choose; until then the first kingdom
    // is the attacker.
    let Some(attacker_kingdom_e) = world
        .get::<CharacterLeads>(actor_e)
        .and_then(|character_leads| character_leads.kingdoms().first().copied())
    else {
        return note(world, "cannot declare war: you rule no kingdom".into());
    };
    let Some(defender_kingdom_e) = world.resource::<Registry>().get(defender_id) else {
        return note(
            world,
            format!("cannot declare war: no such kingdom `{defender_id}`"),
        );
    };
    if defender_kingdom_e == attacker_kingdom_e {
        return note(world, "cannot declare war on yourself".into());
    }
    let Some(cb_type) = resolve_cb(cb_id) else {
        return note(world, format!("unknown casus belli `{cb_id}`"));
    };

    // Capture display names before the spawn (cheap, immutable reads; gives
    // the chronicle line real names instead of bare ids).
    let attacker_name = kingdom_label(world, attacker_kingdom_e);
    let defender_name = kingdom_label(world, defender_kingdom_e);

    // Seed the demands from the CB type + defender. Conquest → one Take
    // demand on the defender kingdom.
    let demands = demands_for(cb_type, defender_kingdom_e);

    // Spawn the war. `WarAttackerKingdom` / `WarDefenderKingdom` are Bevy
    // relationships — the hooks fill the reverses (`KingdomHasWarsAttacking`,
    // `KingdomHasWarsDefending`) synchronously, so any same-frame reader
    // sees authoritative data.
    let war_entity_id = next_id(world);
    // Snapshot the date at declare time so the war carries a stable
    // "declared on" stamp that doesn't drift if the date resource ticks
    // over later. `format_name` reads the CB type + the defender's land
    // name; `Conquest` renders as `"Conquest over Kingdom of <land>"`.
    let begin_date = *world.resource::<Date>();
    let war_name = format_name(world, cb_type, defender_kingdom_e);
    let war_e = world
        .spawn((
            StringId(war_entity_id.clone()),
            War,
            WarAttackerKingdom(attacker_kingdom_e),
            WarDefenderKingdom(defender_kingdom_e),
            cb_type,
            WarDemands(demands),
            WarName(war_name),
            WarBeginDate(begin_date),
        ))
        .id();
    world.resource_mut::<Registry>().insert(war_entity_id, war_e);

    note(
        world,
        format!("{attacker_name} declared war on {defender_name} (conquest)"),
    );
}

/// Display label for a kingdom: the name of its held land, falling back to
/// the kingdom's string id. `KingdomHold` is read for the land entity; the
/// land's `LandName` gives the human-readable label.
fn kingdom_label(world: &World, kingdom_e: bevy::ecs::entity::Entity) -> String {
    world
        .get::<KingdomHold>(kingdom_e)
        .and_then(|kingdom_hold| world.get::<LandName>(kingdom_hold.0))
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| {
            world
                .get::<StringId>(kingdom_e)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| "?".into())
        })
}

/// Format a war's display name from the CB type + the defender kingdom's
/// held land. `Conquest` renders as `"Conquest over Kingdom of <land>"`.
/// New CB shapes are additive: one arm per variant here, one row in the
/// menu in `step_items`, one arm in `resolve_cb`.
fn format_name(
    world: &World,
    cb_type: WarCasusBelliType,
    defender_kingdom_e: bevy::ecs::entity::Entity,
) -> String {
    let land = kingdom_label(world, defender_kingdom_e);
    match cb_type {
        WarCasusBelliType::Conquest => format!("Conquest over Kingdom of {land}"),
    }
}

//! The enforce-demands command: resolve one demand on a war the player is
//! fighting.
//!
//! Two selection steps: step 0 picks a war the player is attacking in
//! (from `KingdomHasWarsAttacking`), step 1 picks one of that war's
//! demands (`WarDemands` list). The pick resolves the demand:
//!
//! - **`WarDemandType::Take`** — only succeeds when the target kingdom's
//!   held land is controlled by one of the player's armies
//!   (`LandControlledByArmy` → army → `ArmyBelongsToKingdom` is the
//!   player's kingdom). On success, the target kingdom's `KingdomLedBy`
//!   is set to the player; Bevy's hook then auto-prunes the old
//!   `KingdomLedBy` and updates `CharacterLeads` on the new and old
//!   leaders.
//!
//! **Bevy's one-to-one leader rule applies.** `CharacterLeads` is a
//! single-`Entity` target on the character (one character leads at most
//! one kingdom), so the `Take` insert that gives the player the target
//! kingdom will drop the player's previous `KingdomLedBy` — the player's
//! old kingdom ends up leaderless. That's the literal "change target
//! kingdom leader to player" semantics; the multi-kingdom model is
//! future work.

use super::core::{note, BaseCommand};
use crate::ecs::{
    ArmyBelongsToKingdom, CharacterLeads, KingdomHasWarsAttacking, KingdomHold,
    LandControlledByArmy, LandName, Registry, WarDemandType, WarDemands, WarName,
};
use crate::ecs::kingdom::KingdomLedBy;
use crate::app::Game;
use crate::ui::command_menu::{CommandHasId, CommandHasKey, CommandHasValue, CommandMenuUiContext};
use bevy::ecs::world::World;
use bevy::prelude::*;
use bevy::prelude::RelationshipTarget;

/// Resolve one demand on a player's war.

// --- palette UI -------------------------------------------------------------
// Same shape as the other commands.

/// Per-row background in the palette.
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
/// Background when the row is the player's selection.
const ROW_PANEL_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
/// Hairline border around the card.
const ROW_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);

impl BaseCommand for EnforceDemands {
    fn get_command_id(&self) -> &'static str {
        "command:enforce_demands"
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

        // Step 1: pick the war.
        let war_pick = choices
            .iter()
            .find(|(k, _)| k == "war_id")
            .map(|(_, v)| v.clone());
        if war_pick.is_none() {
            return self.spawn_war_picker(world, parent);
        }

        // Step 2: pick the demand.
        let demand_pick = choices
            .iter()
            .find(|(k, _)| k == "demand_idx")
            .map(|(_, v)| v.clone());
        if demand_pick.is_none() {
            return self.spawn_demand_picker(world, parent, &war_pick.unwrap());
        }

        // Execute.
        self.execute(world)
    }

    fn update(&self, entity: Entity, is_selected: bool, world: &mut World) {
        let bg = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = bg;
        }
    }
}

impl EnforceDemands {
    fn spawn_command_row(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let row = self.spawn_row(world, parent, "Enforce Demands", None);
        (vec![row], false)
    }

    fn spawn_war_picker(&self, world: &mut World, parent: Entity) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        // Snapshot the wars list.
        let wars = player_wars(world, &actor);
        let mut entities = Vec::new();
        for (war_id, label) in wars {
            let row = self.spawn_row(
                world,
                parent,
                &label,
                Some(("war_id".to_string(), war_id)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn spawn_demand_picker(
        &self,
        world: &mut World,
        parent: Entity,
        war_id: &str,
    ) -> (Vec<Entity>, bool) {
        // Read the war's demands (or use a placeholder if the war is gone).
        let war_e = world.resource::<Registry>().get(war_id);
        let demands_label = match war_e.and_then(|e| world.get::<WarDemands>(e)) {
            Some(wd) if !wd.0.is_empty() => wd
                .0
                .iter()
                .enumerate()
                .map(|(idx, d)| {
                    let shape = match d.demand_type {
                        WarDemandType::Take => "Take",
                    };
                    let target_label = world
                        .get::<KingdomHold>(d.target)
                        .and_then(|kh| world.get::<crate::ecs::LandName>(kh.0))
                        .map(|ln| ln.0.clone())
                        .unwrap_or_else(|| "?".into());
                    (idx.to_string(), format!("{shape} Kingdom of {target_label}"))
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let mut entities = Vec::new();
        for (idx, label) in demands_label {
            let row = self.spawn_row(
                world,
                parent,
                &label,
                Some(("demand_idx".to_string(), idx)),
            );
            entities.push(row);
        }
        (entities, false)
    }

    fn execute(&self, world: &mut World) -> (Vec<Entity>, bool) {
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let picks: Vec<(String, String)> =
            world.resource::<CommandMenuUiContext>().choices.clone();
        let war_id = picks
            .iter()
            .find(|(k, _)| k == "war_id")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a war_id pick");
        let demand_idx = picks
            .iter()
            .find(|(k, _)| k == "demand_idx")
            .map(|(_, v)| v.clone())
            .expect("execute reached without a demand_idx pick");
        enforce(world, &actor, &war_id, &demand_idx);
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
/// Resolve one demand on a player's war.
pub struct EnforceDemands;


/// `(war_id, "<WarName>")` for every war any of the player's kingdoms
/// is attacking in. Multi-kingdom: walks every kingdom the player leads
/// and unions their `KingdomHasWarsAttacking` lists, in
/// `CharacterLeads` order.
fn player_wars(world: &World, actor: &str) -> Vec<(String, String)> {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return Vec::new();
    };
    let Some(character_leads) = world.get::<CharacterLeads>(actor_e) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kingdom_e in character_leads.kingdoms() {
        let Some(khwa) = world.get::<KingdomHasWarsAttacking>(*kingdom_e) else {
            continue;
        };
        for war_e in khwa.iter() {
            let Some(war_id) = world.get::<crate::ecs::StringId>(war_e).map(|s| s.0.clone()) else {
                continue;
            };
            let war_name = world
                .get::<WarName>(war_e)
                .map(|wn| wn.0.clone())
                .unwrap_or_else(|| "?".into());
            out.push((war_id, war_name));
        }
    }
    out
}

/// Resolve the picked demand. `Take` only succeeds if the target
/// kingdom's held land is controlled by one of the player's armies —
/// then the kingdom's `KingdomLedBy` is set to the player.
fn enforce(world: &mut World, actor: &str, war_id: &str, demand_idx: &str) {
    let Some(actor_e) = world.resource::<Registry>().get(actor) else {
        return note(world, "cannot enforce: unknown actor".into());
    };
    // Multi-kingdom: an army under any of the player's kingdoms counts
    // as "yours". The check in `enforce_take` walks the actor's kingdoms
    // and accepts a match on any of them — see there for the actual gate.
    // The `Some(_)` arm just validates the player leads at least one
    // kingdom (so we can refuse the empty case); `enforce_take` walks
    // them itself for the actual ownership check.
    let Some(_actor_kingdoms) = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>())
    else {
        return note(world, "cannot enforce: you rule no kingdom".into());
    };
    let Some(war_e) = world.resource::<Registry>().get(war_id) else {
        return note(world, format!("cannot enforce: no such war `{war_id}`"));
    };
    let Some(w_demands) = world.get::<WarDemands>(war_e) else {
        return note(world, format!("cannot enforce: war `{war_id}` has no demands"));
    };
    let Ok(idx) = demand_idx.parse::<usize>() else {
        return note(world, format!("cannot enforce: bad demand index `{demand_idx}`"));
    };
    let Some(demand) = w_demands.0.get(idx).copied() else {
        return note(world, format!("cannot enforce: demand `{idx}` out of range"));
    };

    // `Take` is the only demand shape today. Match on it; future shapes
    // are additive arms here. On a successful enforcement the war has
    // been resolved — despawn + deregister (mirroring `dismiss_army`'s
    // pattern). Bevy's relationship hooks on `WarAttackerKingdom` /
    // `WarDefenderKingdom` prune the war from both kingdoms'
    // `KingdomHasWarsAttacking` / `KingdomHasWarsDefending` collections
    // as part of the despawn — no manual reverse insert.
    if let Some(crate::ecs::WarDemandType::Take) =
        enforce_take(world, actor_e, demand.target)
    {
        world.despawn(war_e);
        world.resource_mut::<Registry>().by_id.remove(war_id);
        note(world, format!("war `{war_id}` resolved and ended"));
    }
}

/// `Take` — flip the target kingdom's leader to the player. Gate: the
/// target's held land must already be controlled by an army belonging
/// to the player's kingdom (the conquest transfer follows the
/// siege-then-control flow; the demand just enforces the legal transfer
/// step). On success, Bevy's hook on `KingdomLedBy` prunes the old
/// leader's `CharacterLeads` and the player's `CharacterLeads` is
/// adds the entry to the player's `CharacterLeads` `Vec` (multi-kingdom)
/// and prunes the old leader's `CharacterLeads` entry.
fn enforce_take(
    world: &mut World,
    actor_e: bevy::ecs::entity::Entity,
    target_kingdom_e: bevy::ecs::entity::Entity,
) -> Option<crate::ecs::WarDemandType> {
    // Gate: the target kingdom's held land must be controlled by one
    // of the player's armies. Multi-kingdom: any of the actor's
    // kingdoms owning the controlling army counts.
    let target_land = world
        .get::<KingdomHold>(target_kingdom_e)
        .map(|kingdom_hold| kingdom_hold.0);
    let Some(target_land) = target_land else {
        note(
            world,
            "cannot enforce Take: target kingdom has no land".into(),
        );
        return None;
    };
    let Some(controlling_army) = world
        .get::<LandControlledByArmy>(target_land)
        .map(|land_controlled_by_army| land_controlled_by_army.army())
    else {
        note(
            world,
            "cannot enforce Take: target land is not controlled by your army".into(),
        );
        return None;
    };
    let army_kingdom = world
        .get::<ArmyBelongsToKingdom>(controlling_army)
        .map(|army_belongs_to_kingdom| army_belongs_to_kingdom.0);
    let actor_kingdoms = world
        .get::<CharacterLeads>(actor_e)
        .map(|character_leads| character_leads.kingdoms().iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    if !actor_kingdoms.contains(&army_kingdom.unwrap_or(bevy::ecs::entity::Entity::PLACEHOLDER)) {
        note(
            world,
            "cannot enforce Take: target land is not controlled by your army".into(),
        );
        return None;
    }

    // Capture labels before mutating (cheap, immutable reads).
    let target_name = world
        .get::<LandName>(target_land)
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());

    // Insert the new `KingdomLedBy`. Bevy's relationship hook updates
    // `CharacterLeads` on the new leader (player) — under the
    // multi-kingdom model the player can lead the new kingdom AND keep
    // any kingdoms they already led; Bevy adds to the player's
    // `CharacterLeads` Vec instead of replacing (the old leader, if
    // any, has the entry pruned).
    world.entity_mut(target_kingdom_e).insert(KingdomLedBy(actor_e));

    note(
        world,
        format!(
            "took the Kingdom of {target_name} (Take enforced)"
        ),
    );
    Some(crate::ecs::WarDemandType::Take)
}

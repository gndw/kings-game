//! The ACTIONS panel in the right-hand column: context actions for the
//! selected land (build/destroy). Spawned by [`spawn`] between the buildings
//! panel and the chronicle, updated each frame by [`update`].

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::army::ArmyBelongsToKingdom;
use crate::ecs::{CharacterLeads, LandHasArmies, LandHeldBy, Registry};
use bevy::prelude::*;

/// Column container for the actions list. Rebuilt by [`update`] every frame.
#[derive(Component)]
pub struct LegendActions;

/// The ACTIONS panel: title + column container of build/destroy hotkeys.
/// Spawned as a sibling panel between `buildings` and `chronicle` in the
/// right-hand column.
pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        BackgroundColor(panel),
        Node {
            width: percent(100),
            // Size to content; grow only if a child pushes it (flex item
            // default: `flex_grow: 0` + `min_height: Auto`).
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("ACTIONS"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((
            LegendActions,
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(1),
                ..default()
            },
        ));
    });
}

/// One ACTIONS row: a hotkey (in the title colour so it reads as a key) next
/// to the action label.
fn action_row(p: &mut ChildSpawnerCommands, hotkey: &str, label: &str) {
    p.spawn(Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        column_gap: px(6),
        ..default()
    })
    .with_children(|r| {
        r.spawn((
            Text::new(hotkey.to_string()),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        r.spawn((Text::new(label.to_string()), TextFont::from_font_size(FONT)));
    });
}

/// Resolve whether the player rules the currently selected land. `None`
/// selection (or anything that doesn't resolve through registry/queries) is
/// treated as "player doesn't rule it" and yields a `(none)` placeholder.
fn player_rules(
    game: &Game,
    registry: &Registry,
    character_leads: &Query<&CharacterLeads>,
    land_held_by: &Query<&LandHeldBy>,
) -> bool {
    let Some(land_e) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id))
    else {
        return false;
    };
    let player_kingdom = registry
        .get(&game.ctx.player_character_id)
        .and_then(|pe| character_leads.get(pe).ok())
        .map(|character_leads| character_leads.kingdom());
    let land_kingdom = land_held_by
        .get(land_e)
        .ok()
        .map(|land_held_by| land_held_by.kingdom());
    matches!(
        (player_kingdom, land_kingdom),
        (Some(pk), Some(lk)) if pk == lk
    )
}

/// Resolve whether at least one army on the currently selected land belongs
/// to the player's kingdom (via `ArmyBelongsToKingdom`). Gates the **M
/// Dismiss Army** row — the M hotkey dismisses the first such army. Empty
/// selection or "player doesn't rule the land" returns `false`.
fn player_has_army_on_selected_land(
    game: &Game,
    registry: &Registry,
    character_leads: &Query<&CharacterLeads>,
    land_held_by: &Query<&LandHeldBy>,
    land_has_armies: &Query<&LandHasArmies>,
    army_belongs: &Query<&ArmyBelongsToKingdom>,
) -> bool {
    let Some(land_e) = game
        .ctx
        .selected_land_id
        .as_ref()
        .and_then(|id| registry.get(id))
    else {
        return false;
    };
    let Some(player_kingdom) = registry
        .get(&game.ctx.player_character_id)
        .and_then(|pe| character_leads.get(pe).ok())
        .map(|character_leads| character_leads.kingdom())
    else {
        return false;
    };
    if land_held_by
        .get(land_e)
        .ok()
        .map(|land_held_by| land_held_by.kingdom())
        != Some(player_kingdom)
    {
        return false;
    }
    let Ok(land_has_armies) = land_has_armies.get(land_e) else {
        return false;
    };
    land_has_armies.iter().any(|army_e| {
        army_belongs
            .get(army_e)
            .map(|army_belongs| army_belongs.0 == player_kingdom)
            .unwrap_or(false)
    })
}

/// Own system: rebuild the actions list each frame. The list is ≤5 rows, so
/// the despawn/populate cost is negligible — no cache needed.
pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    character_leads: Query<&CharacterLeads>,
    land_held_by: Query<&LandHeldBy>,
    land_has_armies: Query<&LandHasArmies>,
    army_belongs: Query<&ArmyBelongsToKingdom>,
    container: Single<Entity, With<LegendActions>>,
    mut commands: Commands,
) {
    let ruled = player_rules(&game, &registry, &character_leads, &land_held_by);
    let has_army = ruled
        && player_has_army_on_selected_land(
            &game,
            &registry,
            &character_leads,
            &land_held_by,
            &land_has_armies,
            &army_belongs,
        );
    commands.entity(*container).despawn_children();
    commands.entity(*container).with_children(|p| {
        if ruled {
            action_row(p, "b", "Construct Building");
            action_row(p, "d", "Destroy Building");
            action_row(p, "r", "Raise Army");
            if has_army {
                action_row(p, "m", "Dismiss Army");
                action_row(p, "g", "Marching Army");
            }
        } else {
            p.spawn((
                Text::new("(none)"),
                TextFont::from_font_size(FONT),
                TextColor(Color::srgba(0.5, 0.5, 0.5, 0.7)),
            ));
        }
    });
}

//! The WARS panel at the top of the right-hand column: the player's active
//! wars, one per line. Hidden when the player has no wars (via `Display::None`
//! on the outer node so it leaves no gap in the column layout).
//!
//! A "war" is a player-side war: the player's kingdom is the attacker. The
//! list walks `actor → CharacterLeads → kingdom → KingdomHasWarsAttacking` —
//! the auto-maintained reverse of `WarAttackerKingdom`. Insertion order is
//! declare order. Wars change rarely, so the per-frame walk is fine and
//! there's no cache key.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::ecs::{
    CharacterLeads, KingdomHasWarsAttacking, Registry, WarBeginDate, WarName,
};
use bevy::prelude::*;

/// Marker on the WARS panel's body text. The container's visibility is
/// toggled by walking the body's [`Parent`] (Bevy auto-attaches `Parent`
/// when the text is spawned as a child of the panel node), so a single
/// marker covers both the text update and the show/hide.
#[derive(Component)]
pub struct UIWithWars;

pub(super) fn spawn(col: &mut ChildSpawnerCommands, panel: Color) {
    col.spawn((
        // `Display::None` is the default — the player starts with no wars,
        // and `update` flips it to `Flex` on the first declared war.
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(panel),
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("WARS"),
            TextFont::from_font_size(FONT),
            TextColor(TITLE),
        ));
        p.spawn((
            Text::new(""),
            TextFont::from_font_size(FONT),
            TextColor(Color::WHITE),
            UIWithWars,
        ));
    });
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    // Query<(Entity, &mut Text)> so we get the body's entity alongside the
    // mutable text — the parent walk needs the body entity to find the
    // container. Iterate, don't `single_mut`: the body should always be
    // there (spawned at startup), but a `for` loop degrades gracefully if
    // it isn't, whereas `single_mut` would panic.
    mut bodies: Query<(Entity, &mut Text), With<UIWithWars>>,
    // `ChildOf` is Bevy 0.19's renamed `Parent` component (this is what
    // every UI child gets auto-spawned with). The body text's parent is
    // the panel's container node — toggling its `Display` hides the whole
    // panel.
    parents: Query<&ChildOf>,
    mut nodes: Query<&mut Node>,
    wars: Query<(&WarName, &WarBeginDate)>,
    player_chars: Query<&CharacterLeads>,
    kingdom_wars: Query<&KingdomHasWarsAttacking>,
) {
    // Player → kingdoms → wars. Any miss short-circuits to "no wars"
    // and hides the panel — including the case where the player
    // hasn't been resolved yet (fresh world, no player character
    // entity). Multi-kingdom: union every kingdom the player leads.
    let war_lines: Vec<String> = game
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| registry.get(id))
        .and_then(|player_e| player_chars.get(player_e).ok())
        .map(|character_leads| {
            let mut out = Vec::new();
            for kingdom_e in character_leads.kingdoms() {
                let Some(kingdom_has_wars) = kingdom_wars.get(*kingdom_e).ok() else {
                    continue;
                };
                for war_e in kingdom_has_wars.iter() {
                    if let Ok((name, begin)) = wars.get(war_e) {
                        out.push(format!("{} ({})", name.0, begin.0));
                    }
                }
            }
            out
        })
        .unwrap_or_default();

    // Toggle the outer container's `Display`. `Display::None` removes the
    // panel from layout so the gap above `INFORMATION` shrinks to zero
    // when the player has no wars; `Display::Flex` puts it back.
    let visible = !war_lines.is_empty();
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    for (body_e, mut body) in &mut bodies {
        if let Ok(child_of) = parents.get(body_e)
            && let Ok(mut node) = nodes.get_mut(child_of.parent())
        {
            node.display = display;
        }
        body.0 = war_lines.join("\n");
    }
}

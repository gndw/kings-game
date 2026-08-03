//! The resource bar along the top: who the player is and what they hold.
//! Mirrors the status bar at the bottom of the screen.

use super::FONT;
use crate::app::Game;
use crate::ecs::{CharacterGold, CharacterGoldYield, CharacterLevy, CharacterName, HouseName, HouseOf, Registry};
use bevy::prelude::*;

/// Named `ResourceBar`, not `Resource` — that one is Bevy's trait.
#[derive(Component)]
pub struct ResourceBar;

pub(super) fn spawn(root: &mut ChildSpawnerCommands, panel: Color) {
    root.spawn((
        ResourceBar,
        Text::new(""),
        TextFont::from_font_size(FONT),
        TextLayout::justify(Justify::Center),
        BackgroundColor(panel),
        Node {
            width: percent(100),
            justify_content: JustifyContent::Center,
            padding: UiRect::all(px(3)),
            ..default()
        },
    ));
}

pub fn update(
    game: Res<Game>,
    registry: Res<Registry>,
    mut bar: Single<&mut Text, With<ResourceBar>>,
    chars: Query<(&CharacterName, &CharacterGold, &CharacterGoldYield, &CharacterLevy)>,
    house_of: Query<&HouseOf>,
    houses: Query<&HouseName>,
) {
    // A map that doesn't contain the player leaves the bar blank rather than
    // showing zeroes that look like a broke ruler.
    let Some(player_e) = registry.get(&game.ctx.player_character_id) else {
        bar.0 = String::new();
        return;
    };
    let Ok((ch, gold, gold_yield, levy)) = chars.get(player_e) else {
        bar.0 = String::new();
        return;
    };
    let house = house_of
        .get(player_e)
        .ok()
        .and_then(|ho| houses.get(ho.0).ok())
        .map(|h| h.0.clone())
        .unwrap_or_default();
    // The monthly income the gold script last published — it owns the rule, so
    // a mod that changes how income is figured changes this number with it.
    // Signed both places: a realm can run at a loss and a ruler can be in debt.
    bar.0 = format!(
        "{} of {}     {} gold ({:+}/mo)     {} levy",
        ch.0, house, gold.0, gold_yield.0, levy.0
    );
}

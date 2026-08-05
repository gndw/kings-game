//! The resource bar along the top: who the player is and what they hold.
//! Mirrors the status bar at the bottom of the screen.

use super::FONT;
use crate::app::Game;
use crate::ecs::{
    CharacterGold, CharacterGoldYield, CharacterLevy, CharacterName, CharacterOfHouse, HouseName,
    Registry,
};
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
    character_of_house: Query<&CharacterOfHouse>,
    houses: Query<&HouseName>,
) {
    // A map that doesn't contain the player leaves the bar blank rather than
    // showing zeroes that look like a broke ruler.
    let Some(player_e) = registry.get(&game.ctx.player_character_id) else {
        bar.0 = String::new();
        return;
    };
    let Ok((character_name, character_gold, character_gold_yield, character_levy)) =
        chars.get(player_e)
    else {
        bar.0 = String::new();
        return;
    };
    let house = character_of_house
        .get(player_e)
        .ok()
        .and_then(|character_of_house| houses.get(character_of_house.0).ok())
        .map(|house_name| house_name.0.clone())
        .unwrap_or_default();
    // The monthly income the gold script last published — it owns the rule, so
    // a mod that changes how income is figured changes this number with it.
    // Signed both places: a realm can run at a loss and a ruler can be in debt.
    bar.0 = format!(
        "{} of {}     {} gold ({:+}/mo)     {} levy",
        character_name.0,
        house,
        character_gold.0,
        character_gold_yield.0,
        character_levy.0
    );
}

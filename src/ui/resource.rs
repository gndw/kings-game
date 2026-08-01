//! The resource bar along the top: who the player is and what they hold.
//! Mirrors the status bar at the bottom of the screen.

use super::FONT;
use crate::app::Game;
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

pub fn update(game: Res<Game>, mut bar: Single<&mut Text, With<ResourceBar>>) {
    // A map that doesn't contain the player leaves the bar blank rather than
    // showing zeroes that look like a broke ruler.
    let Some(player) = game.ctx.player_character() else {
        bar.0 = String::new();
        return;
    };
    let house = game
        .ctx
        .content
        .house(&player.house_id)
        .map_or(player.house_id.as_str(), |h| h.name.as_str());
    // Profit, not profit-minus-upkeep, because that is what the base script
    // actually pays out each month. A mod that changes the rule should change
    // this line too.
    let income = game.ctx.yield_for(&player.id).gold_profit;
    bar.0 = format!(
        "{} of {}     {} gold (+{}/mo)     {} levy",
        player.name, house, player.gold, income, player.levy
    );
}

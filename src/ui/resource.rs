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
    let Some(p) = game.ctx.player_summary() else {
        bar.0 = String::new();
        return;
    };
    // The monthly income the gold script last published — it owns the rule, so
    // a mod that changes how income is figured changes this number with it.
    // Signed both places: a realm can run at a loss and a ruler can be in debt.
    bar.0 = format!(
        "{} of {}     {} gold ({:+}/mo)     {} levy",
        p.name, p.house, p.gold, p.gold_yield, p.levy
    );
}

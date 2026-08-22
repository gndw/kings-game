//! The resource bar along the top: who the player is and what they hold.
//! Mirrors the status bar at the bottom of the screen.

use super::FONT;
use crate::app::Game;
use crate::ecs::{
    CharacterName, CharacterOfHouse, HouseName, KingdomGold, KingdomGoldYield, KingdomLevy,
    Registry,
};
use crate::helper::kingdom_helper::get_character_ruled_kingdoms;
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

pub fn update(world: &mut World) {
    let Some(bar) = world
        .query_filtered::<Entity, With<ResourceBar>>()
        .iter(world)
        .next()
    else {
        return;
    };

    // A map that doesn't contain the player leaves the bar blank rather than
    // showing zeroes that look like a broke ruler.
    let Some(player_e) = world
        .resource::<Game>()
        .ctx
        .player_character_id
        .as_deref()
        .and_then(|id| world.resource::<Registry>().get(id))
    else {
        if let Some(mut text) = world.get_mut::<Text>(bar) {
            text.0 = String::new();
        }
        return;
    };

    let character_name = world
        .get::<CharacterName>(player_e)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let house = world
        .get::<CharacterOfHouse>(player_e)
        .and_then(|coh| world.get::<HouseName>(coh.0))
        .map(|hn| hn.0.clone())
        .unwrap_or_default();

    // Sum the player's gold, gold_yield, and levy across every kingdom they
    // rule. The top bar reads as a single number to keep the medieval
    // "how rich am I" feel; the kingdom panel drills into a single kingdom.
    let mut gold: i64 = 0;
    let mut gold_yield: i64 = 0;
    let mut levy: u64 = 0;
    for ke in get_character_ruled_kingdoms(world, player_e) {
        gold += world.get::<KingdomGold>(ke).map(|g| g.0).unwrap_or(0);
        gold_yield += world.get::<KingdomGoldYield>(ke).map(|g| g.0).unwrap_or(0);
        levy += world.get::<KingdomLevy>(ke).map(|l| l.0).unwrap_or(0);
    }
    // The monthly income the gold script last published — it owns the rule, so
    // a mod that changes how income is figured changes this number with it.
    // Signed both places: a realm can run at a loss and a ruler can be in debt.
    let text = format!(
        "{} of {}     {} gold ({:+}/mo)     {} levy",
        character_name, house, gold, gold_yield, levy
    );
    if let Some(mut t) = world.get_mut::<Text>(bar) {
        t.0 = text;
    }
}

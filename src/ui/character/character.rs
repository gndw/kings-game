//! Character panel core: types, resources, the shell spawner, the input
//! handler, and the per-frame renderer. Each rendered section lives in its
//! own submodule; this file owns the orchestrator (`render_character_spans`).

use crate::app::Game;
use crate::ecs::character::{
    CharacterDateOfBirth, CharacterFaith, CharacterGender, CharacterGold, CharacterGoldYield,
    CharacterIntrigue, CharacterLeads, CharacterLevy, CharacterMartial, CharacterName,
    CharacterOfHouse, CharacterProwess, CharacterPrudence, CharacterTreasury,
};
use crate::ecs::house::HouseName;
use crate::ecs::kingdom::{KingdomLedBy, KingdomName};
use crate::ecs::{Registry, StringId};
use crate::helper::age_helper::age;
use crate::helper::opinion_helper::opinion_of_via_world;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

use super::super::spawn_span;
use super::character_detail::render_detail_spans;
use super::character_skills::render_skills_spans;
use super::character_stats::render_stats_spans;

/// The full panel shell (root node). Hidden by default; flipped open by
/// `input` when the player presses R on the pinned kingdom's ruler.
#[derive(Component)]
pub struct CharacterUIRoot;

/// The dynamic body of the panel — every section under the title lives here.
#[derive(Component)]
pub struct CharacterUIBody;

/// Tracks which character the panel is currently showing, if any. Lives in
/// the world so the input handler and the renderer can each read it without
/// walking the UI node tree. The pinned kingdom lives separately in
/// [`crate::ui::kingdom::KingdomUiContext`]; while the character panel is
/// open the kingdom stays pinned so **Backspace** can pop back.
#[derive(Resource, Default)]
pub struct CharacterUiContext {
    pub character_id: Option<String>,
}


/// Key handler gated to the root input layer. **R** opens the ruler of the
/// pinned kingdom (no-op when nothing's pinned); **Enter** closes both
/// panels; **Backspace** closes only the character panel, leaving the
/// kingdom panel still pinned.
///
/// Exclusive: the toggle pokes two resources and pokes the UI tree in
/// separate worlds; Bevy exclusive access only routes through `&mut World`.
pub fn input(world: &mut World) {
    if *world.resource::<InputLayer>() != InputLayer::Root {
        return;
    }
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let r_pressed = keys.just_pressed(KeyCode::KeyR);
    let enter_pressed = keys.just_pressed(KeyCode::Enter);
    let backspace_pressed = keys.just_pressed(KeyCode::Backspace);

    let char_open = world
        .resource::<CharacterUiContext>()
        .character_id
        .is_some();

    if r_pressed && !char_open {
        open_ruler(world);
        return;
    }
    if enter_pressed && char_open {
        close_all(world);
        return;
    }
    if backspace_pressed && char_open {
        close_character_only(world);
        return;
    }
}

/// Resolve the pinned kingdom's ruler and set `character_id` to it. No-op
/// when nothing is pinned, the kingdom is gone, or it has no ruler. Hides
/// the kingdom shell so the two panels don't stack at the same coords.
fn open_ruler(world: &mut World) {
    let pinned_id = world
        .resource::<crate::ui::kingdom::KingdomUiContext>()
        .pinned_kingdom_id
        .clone();
    let Some(kingdom_id) = pinned_id else {
        return;
    };
    let Some(kingdom_e) = world.resource::<Registry>().get(&kingdom_id) else {
        return;
    };
    let Some(ruler_e) = world.get::<KingdomLedBy>(kingdom_e).map(|k| k.0) else {
        return;
    };
    let Some(char_id) = world.get::<StringId>(ruler_e).map(|s| s.0.clone()) else {
        return;
    };
    world.resource_mut::<CharacterUiContext>().character_id = Some(char_id);
    set_visible(world, true);
    crate::ui::kingdom::set_visible(world, false);
}

/// Close both panels: character + kingdom. Used by **Enter** from the
/// character panel — "exit this drill-down entirely".
fn close_all(world: &mut World) {
    world.resource_mut::<CharacterUiContext>().character_id = None;
    world.resource_mut::<crate::ui::kingdom::KingdomUiContext>().pinned_kingdom_id = None;
    set_visible(world, false);
    crate::ui::kingdom::set_visible(world, false);
}

/// Close only the character panel; kingdom stays pinned and re-shown.
/// Used by **Backspace** from the character panel — "go back one step".
fn close_character_only(world: &mut World) {
    world.resource_mut::<CharacterUiContext>().character_id = None;
    set_visible(world, false);
    if world
        .resource::<crate::ui::kingdom::KingdomUiContext>()
        .pinned_kingdom_id
        .is_some()
    {
        crate::ui::kingdom::set_visible(world, true);
    }
}

fn set_visible(world: &mut World, visible: bool) {
    let Some(root) = world
        .query_filtered::<Entity, With<CharacterUIRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = if visible { Display::Flex } else { Display::None };
    }
}

/// Rebuild the body text every frame from the open character's live data.
/// Exclusive because the skill query is a small fanout but the body render
/// fans out further; using `&mut World` keeps the param count under Bevy's
/// 16-tuple ceiling the way the kingdom panel does.
pub fn update(world: &mut World) {
    let Some(body_e) = world
        .query_filtered::<Entity, With<CharacterUIBody>>()
        .iter(world)
        .next()
    else {
        return;
    };

    let char_id = world.resource::<CharacterUiContext>().character_id.clone();
    let Some(char_id) = char_id else {
        world.entity_mut(body_e).despawn_children();
        return;
    };
    let Some(char_e) = world.resource::<Registry>().get(&char_id) else {
        world.resource_mut::<CharacterUiContext>().character_id = None;
        world.entity_mut(body_e).despawn_children();
        return;
    };

    let player_e = {
        let game = world.resource::<Game>();
        let registry = world.resource::<Registry>();
        game.ctx
            .player_character_id
            .as_deref()
            .and_then(|id| registry.get(id))
    };

    let spans = render_character_spans(world, char_e, player_e);
    world.entity_mut(body_e).despawn_children();
    world.commands().entity(body_e).with_children(|p| {
        for (text, color) in spans {
            spawn_span(p, text, color);
        }
    });
}

/// Build every visible span for the open character. Each sub-renderer is a
/// pure `(text, color)` builder; the caller iterates and spawns.
fn render_character_spans(
    world: &mut World,
    char_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    // Snapshot every value we need from the character entity first; this
    // drops the immutable borrow of `world` so we can later call
    // `opinion_of_via_world` (which needs `&mut World`) without a conflict.
    let (name, house, gender, char_age, kingdom_name, gold, gold_yield, levy, skills) = {
        let ent = world.entity(char_e);
        let Some(name) = ent.get::<CharacterName>().map(|n| n.0.clone()) else {
            return Vec::new();
        };
        let Some(dob) = ent.get::<CharacterDateOfBirth>() else {
            return Vec::new();
        };
        let Some(gender) = ent.get::<CharacterGender>().copied() else {
            return Vec::new();
        };
        let house = ent
            .get::<CharacterOfHouse>()
            .and_then(|cof| world.entity(cof.0).get::<HouseName>())
            .map(|hn| hn.0.clone())
            .unwrap_or_default();
        let char_age = age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
        let kingdom_name = ent
            .get::<CharacterLeads>()
            .and_then(|led| led.kingdoms().first().copied())
            .and_then(|k_e| world.entity(k_e).get::<KingdomName>())
            .map(|kn| kn.0.clone());
        let gold = ent.get::<CharacterGold>().copied().unwrap_or_default().0;
        let gold_yield = ent.get::<CharacterGoldYield>().copied().unwrap_or_default().0;
        let levy = ent.get::<CharacterLevy>().copied().unwrap_or_default().0;
        let skills = (
            ent.get::<CharacterMartial>().copied().unwrap_or_default().0,
            ent.get::<CharacterProwess>().copied().unwrap_or_default().0,
            ent.get::<CharacterTreasury>().copied().unwrap_or_default().0,
            ent.get::<CharacterPrudence>().copied().unwrap_or_default().0,
            ent.get::<CharacterIntrigue>().copied().unwrap_or_default().0,
            ent.get::<CharacterFaith>().copied().unwrap_or_default().0,
        );
        (name, house, gender, char_age, kingdom_name, gold, gold_yield, levy, skills)
    };

    // Opinion needs &mut World (opinion_of_via_world does); compute after
    // the immutable borrow of `world.entity(char_e)` is dropped.
    let opinion = player_e.filter(|p| *p != char_e).map(|player| {
        let date = world.resource::<Date>().clone();
        opinion_of_via_world(world, char_e, player, &date)
    });

    let mut spans: Vec<(String, Color)> = Vec::new();
    spans.extend(render_detail_spans(
        &name,
        &house,
        gender,
        char_age,
        kingdom_name.as_deref(),
        opinion,
    ));
    spans.extend(render_stats_spans(gold, gold_yield, levy));
    spans.extend(render_skills_spans(skills));
    spans
}

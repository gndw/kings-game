//! Kingdom panel core: types, resources, the shell spawner, the input
//! handler, and the per-frame renderer. Each rendered section lives in its
//! own submodule; this file owns the orchestrator (`render_kingdom_spans`).

use crate::app::Game;
use crate::ecs::{KingdomHold, KingdomName, LandHeldBy, Registry, StringId};
use crate::helper::kingdom_helper::get_kingdom_ruler;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

use super::super::spawn_span;
use super::kingdom_army::render_armies_spans;
use super::kingdom_buildings::render_buildings_spans;
use super::kingdom_courts::render_courtiers_spans;
use super::kingdom_detail::{render_land_spans, render_name_spans, render_ruler_spans};
use super::kingdom_war::render_wars_spans;

/// The full panel shell (root node). Hidden by default; flipped open by
/// `apply_toggle` once the player presses Enter on a kingdom.
#[derive(Component)]
pub struct KingdomUIRoot;

/// The dynamic body of the panel — every section under the title lives here.
#[derive(Component)]
pub struct KingdomUIBody;

/// Tracks which kingdom the panel is currently pinned to, if any. Lives in
/// the world so the input handler and the renderer can each read it without
/// walking the UI node tree.
#[derive(Resource, Default)]
pub struct KingdomUiContext {
    pub pinned_kingdom_id: Option<String>,
}

/// Run condition: the root input layer is active (no modal in the way).
/// Exposed so `main.rs` can attach it via `run_if`, but the same check also
/// sits inside [`input`] — defense in depth, so a future system re-ordering
/// can't silently fire Enter on the kingdom while the palette is open.
pub fn root_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Root
}

/// Enter at the root layer toggles the panel: same kingdom → close,
/// different kingdom → switch, no panel → open on the selected land's
/// kingdom. Gated to `InputLayer::Root` inline so the run-if in `main.rs`
/// isn't the sole gate — defensive against any future system reorder.
/// Exclusive because the toggle pokes the resource and the UI tree in
/// one shot; Bevy exclusive access only routes through `&mut World`.
pub fn input(world: &mut World) {
    if *world.resource::<InputLayer>() != InputLayer::Root {
        return;
    }
    let enter = world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Enter);
    if !enter {
        return;
    }
    let Some(target_e) = selected_kingdom(world) else {
        return;
    };
    let target_id = world
        .get::<StringId>(target_e)
        .map(|s| s.0.clone())
        .unwrap_or_default();
    let action = match world.resource::<KingdomUiContext>().pinned_kingdom_id.clone() {
        Some(id) if id == target_id => Toggle::Close,
        Some(_) => Toggle::Switch(target_id),
        None => Toggle::Open(target_id),
    };
    apply_toggle(world, action);
}

/// What `apply_toggle` will do — encoded so the action choice and the
/// application stay in one place each.
enum Toggle {
    Close,
    Open(String),
    Switch(String),
}

fn apply_toggle(world: &mut World, action: Toggle) {
    match action {
        Toggle::Close => {
            world.resource_mut::<KingdomUiContext>().pinned_kingdom_id = None;
            set_visible(world, false);
        }
        Toggle::Open(id) | Toggle::Switch(id) => {
            world.resource_mut::<KingdomUiContext>().pinned_kingdom_id = Some(id);
            set_visible(world, true);
        }
    }
}

/// Toggle the kingdom panel shell's visibility. `pub(crate)` so the
/// character panel can hide this shell while its drill-down replaces the
/// slot — both panels occupy the same right-docked 35% area and only one
/// should be on-screen at a time.
pub(crate) fn set_visible(world: &mut World, visible: bool) {
    let Some(root) = world
        .query_filtered::<Entity, With<KingdomUIRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = if visible { Display::Flex } else { Display::None };
    }
}

/// The kingdom holding the currently selected land. `None` when nothing's
/// selected or the land has no holder — `input` no-ops on `None` so the
/// player can't open an empty panel.
fn selected_kingdom(world: &World) -> Option<Entity> {
    let game = world.resource::<Game>();
    let registry = world.resource::<Registry>();
    let land_e = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))?;
    world.get::<LandHeldBy>(land_e).map(|lh| lh.kingdom())
}

/// Rebuild the body text every frame from the pinned kingdom's live data.
/// Exclusive because 30+ system parameters would exceed Bevy's 16-tuple
/// param ceiling; helpers fetch data via `&World` instead.
pub fn update(world: &mut World) {
    // Skip while the character drill-down is replacing this panel. The
    // character panel's `input` flips our shell to `display: None` so the
    // body is invisible anyway; rebuilding it would be wasted work.
    if world
        .resource::<super::super::character::CharacterUiContext>()
        .character_id
        .is_some()
    {
        return;
    }
    let Some(body_e) = world
        .query_filtered::<Entity, With<KingdomUIBody>>()
        .iter(world)
        .next()
    else {
        return;
    };

    let pinned_id = world.resource::<KingdomUiContext>().pinned_kingdom_id.clone();
    let Some(pinned_id) = pinned_id else {
        world.entity_mut(body_e).despawn_children();
        return;
    };
    let Some(kingdom_e) = world.resource::<Registry>().get(&pinned_id) else {
        world.entity_mut(body_e).despawn_children();
        return;
    };

    // Pull the kingdom's metadata up front; everything below takes &World.
    let (kingdom_name, ruler_e, kingdom_hold) = {
        let ent = world.entity(kingdom_e);
        let n = ent.get::<KingdomName>().map(|c| c.0.clone());
        let h = ent.get::<KingdomHold>().map(|c| c.0);
        (n, get_kingdom_ruler(world, kingdom_e), h)
    };
    let Some(kingdom_name) = kingdom_name else {
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

    let spans = render_kingdom_spans(world, kingdom_name, kingdom_hold, ruler_e, kingdom_e, player_e);

    world.entity_mut(body_e).despawn_children();
    world.commands().entity(body_e).with_children(|p| {
        for (text, color) in spans {
            spawn_span(p, text, color);
        }
    });
}

/// Build every visible span for the pinned kingdom. Each sub-renderer is a
/// pure `(text, color)` builder; the caller iterates and spawns.
fn render_kingdom_spans(
    world: &mut World,
    name: String,
    land_e: Option<Entity>,
    ruler_e: Option<Entity>,
    kingdom_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let mut spans: Vec<(String, Color)> = Vec::new();
    spans.extend(render_name_spans(&name));
    if let Some(land_e) = land_e {
        spans.extend(render_land_spans(world, land_e));
        spans.extend(render_buildings_spans(world, land_e));
    }
    if let Some(ruler_e) = ruler_e {
        spans.extend(render_ruler_spans(world, ruler_e, player_e));
    }
    spans.extend(render_courtiers_spans(world, kingdom_e, player_e));
    spans.extend(render_wars_spans(world, kingdom_e));
    spans.extend(render_armies_spans(world, kingdom_e));
    spans
}

//! The command palette: panel shell + open/close + search bar. Each command's
//! UI is spawned by `commands::core::spawn_command`; this module owns the
//! panel's container, the search bar, the visibility/layer transitions, and
//! the search filter.

use bevy::ecs::message::MessageCursor;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::resources::input_layer::InputLayer;

// --- shell components -------------------------------------------------------

#[derive(Component)]
pub struct CommandMenuUIRoot;
/// Wraps the search bar + scrollable list; owns the border and background.
#[derive(Component)]
pub struct CommandMenuUIWindow;
/// The scrollable inner list; owns the row children.
#[derive(Component)]
pub struct CommandMenuUIList;
/// The search bar container above the list.
#[derive(Component)]
pub struct MenuSearch;
/// The text node inside the search bar; rewritten from `query` every frame.
#[derive(Component)]
pub struct MenuSearchText;

// --- row metadata ----------------------------------------------------------

/// Command id stamped on every row by the spawning command; the orchestrator
/// dispatches the selection visual only to the matching command.
#[derive(Component, Clone, Debug)]
pub struct CommandHasId(pub String);
/// Search key stamped on every row; the palette matches the query against
/// this string (case-insensitive substring). Rows without it are "always matches".
#[derive(Component, Clone, Debug)]
pub struct CommandHasQueryable(pub String);
/// The row's name-text child entity, so the palette can recolour just the name.
#[derive(Component, Clone, Debug)]
pub struct RowNameText(pub Entity);
/// Key tag on a step row, paired with `CommandHasValue`.
#[derive(Component, Clone, Debug)]
pub struct CommandHasKey(pub String);
/// Value tag on a step row — the concrete id the row represents.
#[derive(Component, Clone, Debug)]
pub struct CommandHasValue(pub String);

// --- context ---------------------------------------------------------------

/// Per-open state the palette exposes to the rest of the game.
#[derive(Resource, Default)]
pub struct CommandMenuUiContext {
    pub item_entities: Vec<Entity>,
    pub selected_index: i32,
    pub choices: Vec<(String, String)>,
    pub query: String,
    pub matches: Vec<bool>,
    pub cursor: MessageCursor<KeyboardInput>,
}

// --- styling ---------------------------------------------------------------

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const SEARCH_BG: Color = Color::srgb(0.06, 0.06, 0.08);
const SEARCH_BORDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.35);
const SEARCH_TEXT: Color = Color::srgb(0.96, 0.96, 0.96);
const SEARCH_PLACEHOLDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.85);
const ROW_PANEL_GRAYED: Color = Color::srgb(0.12, 0.12, 0.15);
const NAME_COLOR_GRAYED: Color = Color::srgba(0.96, 0.96, 0.98, 0.35);

// --- scroll math ----------------------------------------------------------

const LIST_PADDING: f32 = 10.0;
const LIST_ROW_GAP: f32 = 6.0;
const SCROLL_MARGIN: f32 = 8.0;

// --- startup --------------------------------------------------------------

/// Spawn the palette's panel shell once, hidden.
pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            CommandMenuUIRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(BACKDROP),
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                CommandMenuUIWindow,
                Node {
                    width: percent(45),
                    max_height: percent(70),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(8),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(BORDER),
            ))
            .with_children(|window| {
                window
                    .spawn((
                        MenuSearch,
                        Node {
                            width: percent(100),
                            padding: UiRect::all(px(8)),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(4)),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(SEARCH_BG),
                        BorderColor::all(SEARCH_BORDER),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            MenuSearchText,
                            Text::new(""),
                            TextFont::from_font_size(16.0),
                            TextColor(SEARCH_TEXT),
                        ));
                    });
                window.spawn((
                    CommandMenuUIList,
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(LIST_PADDING)),
                        row_gap: px(LIST_ROW_GAP),
                        overflow: Overflow::scroll_y(),
                        flex_grow: 1.0,
                        min_height: px(0.0),
                        ..default()
                    },
                    ScrollPosition::default(),
                ));
            });
        });
}

// --- open / close ---------------------------------------------------------

/// Show the panel, populate it via the data-side orchestrator, set the input layer.
pub fn open_command(world: &mut World) {
    show_panel(world);
    let (item_entities, executed) = crate::commands::core::spawn_command(world, &[]);
    {
        // Advance the cursor past events that were already in the stream when
        // the player pressed C, so the "c" keypress doesn't sneak into the search bar.
        world.resource_scope(
            |world: &mut World, mut messages: Mut<Messages<KeyboardInput>>| {
                let mut context = world.resource_mut::<CommandMenuUiContext>();
                context.item_entities = item_entities;
                context.query.clear();
                context.matches.clear();
                context.cursor.clear(&mut *messages);
            },
        );
    }
    refresh(world);
    *world.resource_mut::<InputLayer>() = InputLayer::CommandMenu;

    if executed {
        close_command(world);
    }
}

/// Despawn every spawned row, clear the context, hide the panel, restore the input layer.
pub fn close_command(world: &mut World) {
    despawn_command_rows(world);
    hide_panel(world);
    let mut context = world.resource_mut::<CommandMenuUiContext>();
    context.item_entities.clear();
    context.selected_index = -1;
    context.choices.clear();
    context.query.clear();
    context.matches.clear();
    *world.resource_mut::<InputLayer>() = InputLayer::Root;
}

fn show_panel(world: &mut World) {
    let Some(root) = world
        .query_filtered::<Entity, With<CommandMenuUIRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = Display::Flex;
    }
}

fn hide_panel(world: &mut World) {
    let Some(root) = world
        .query_filtered::<Entity, With<CommandMenuUIRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = Display::None;
    }
}

/// Despawn every row tracked in `item_entities`.
fn despawn_command_rows(world: &mut World) {
    let entities = world.resource::<CommandMenuUiContext>().item_entities.clone();
    for e in entities {
        world.despawn(e);
    }
}

// --- input -----------------------------------------------------------------

/// Run condition: the command-menu input layer is active.
pub fn command_menu_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::CommandMenu
}

/// Per-frame input handler: drain typed chars, mirror the search bar, handle Esc/Enter/arrows.
pub fn input(world: &mut World) {
    drain_typed_input(world);
    mirror_search_text(world);

    let keys = world.resource::<ButtonInput<KeyCode>>();
    if keys.just_released(KeyCode::Escape) {
        close_command(world);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) {
        handle_enter(world);
        return;
    }
    navigation(world);
}

/// Drain `Messages<KeyboardInput>`, mutating `query` for Backspace and typed chars.
fn drain_typed_input(world: &mut World) {
    let mut cursor = world.resource::<CommandMenuUiContext>().cursor.clone();
    let mut typed: Vec<KeyboardInput> = Vec::new();
    {
        let mut messages = world.resource_mut::<Messages<KeyboardInput>>();
        for event in cursor.read(&mut *messages) {
            if event.state != ButtonState::Pressed {
                continue;
            }
            if event.key_code == KeyCode::Backspace {
                typed.push(event.clone());
            } else if matches!(event.logical_key, Key::Character(_) | Key::Space) {
                typed.push(event.clone());
            }
        }
    }
    if typed.is_empty() {
        // Still write the cursor back so it advances past events we ignored.
        world.resource_mut::<CommandMenuUiContext>().cursor = cursor;
        return;
    }
    {
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        for event in &typed {
            if event.key_code == KeyCode::Backspace {
                ctx.query.pop();
            } else if let Some(text) = event.text.as_deref() {
                for ch in text.chars() {
                    ctx.query.push(ch);
                }
            }
        }
        ctx.cursor = cursor;
    }
    refresh(world);
}

/// Mirror `query` into the `MenuSearchText` node — placeholder when empty, prefixed verbatim otherwise.
fn mirror_search_text(world: &mut World) {
    let query = world.resource::<CommandMenuUiContext>().query.clone();
    let Some(text_entity) = world
        .query_filtered::<Entity, With<MenuSearchText>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let (new_text, color) = if query.is_empty() {
        ("Search...".to_string(), SEARCH_PLACEHOLDER)
    } else {
        (format!("> {query}"), SEARCH_TEXT)
    };
    if let Some(mut text) = world.get_mut::<Text>(text_entity) {
        text.0 = new_text;
    }
    if let Some(mut text_color) = world.get_mut::<TextColor>(text_entity) {
        text_color.0 = color;
    }
}

/// Enter: capture the selected row's metadata into `choices`, despawn all rows,
/// re-spawn against the updated choices. No-op when no row is selected.
fn handle_enter(world: &mut World) {
    if world.resource::<CommandMenuUiContext>().selected_index < 0 {
        return;
    }
    let selected_entity: Option<Entity> = {
        let context = world.resource::<CommandMenuUiContext>();
        let idx = context.selected_index;
        if idx < 0 {
            None
        } else {
            context.item_entities.get(idx as usize).copied()
        }
    };
    if let Some(entity) = selected_entity {
        // Pick is sticky once made.
        if let Some(id) = world.get::<CommandHasId>(entity).map(|c| c.0.clone()) {
            let mut context = world.resource_mut::<CommandMenuUiContext>();
            if !context.choices.iter().any(|(k, _)| k == "command") {
                context.choices.push(("command".to_string(), id));
            }
        }
        if let (Some(key), Some(value)) = (
            world.get::<CommandHasKey>(entity).map(|c| c.0.clone()),
            world.get::<CommandHasValue>(entity).map(|c| c.0.clone()),
        ) {
            let mut context = world.resource_mut::<CommandMenuUiContext>();
            context.choices.push((key, value));
        }
    }

    let entities: Vec<Entity> = world
        .resource::<CommandMenuUiContext>()
        .item_entities
        .clone();
    for e in entities {
        world.despawn(e);
    }

    let choices = world.resource::<CommandMenuUiContext>().choices.clone();
    let (new_entities, executed) = crate::commands::core::spawn_command(world, &choices);

    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.item_entities = new_entities;
        context.query.clear();
        context.matches.clear();
    }
    refresh(world);

    if executed {
        close_command(world);
    }
}

/// Arrow-key navigation: move the cursor by one (wrapping), with a non-empty
/// query the cursor walks only the matches.
fn navigation(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    if !up && !down {
        return;
    }

    let (item_entities, current, matches, query) = {
        let context = world.resource::<CommandMenuUiContext>();
        (
            context.item_entities.clone(),
            context.selected_index,
            context.matches.clone(),
            context.query.clone(),
        )
    };
    if item_entities.is_empty() {
        return;
    }

    let new_index = if !query.is_empty() {
        let match_indices: Vec<usize> = matches
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.then_some(i))
            .collect();
        if match_indices.is_empty() {
            return;
        }
        let current_pos = match_indices
            .iter()
            .position(|&i| (i as i32) == current)
            .unwrap_or(0);
        let len = match_indices.len();
        let new_pos = if up {
            if current_pos == 0 { len - 1 } else { current_pos - 1 }
        } else {
            if current_pos == len - 1 { 0 } else { current_pos + 1 }
        };
        match_indices[new_pos] as i32
    } else {
        let len = item_entities.len() as i32;
        if up {
            if current <= 0 { len - 1 } else { current - 1 }
        } else {
            if current >= len - 1 { 0 } else { current + 1 }
        }
    };

    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.selected_index = new_index;
    }

    for (i, entity) in item_entities.iter().enumerate() {
        let selected = (i as i32) == new_index;
        let grayed = !matches.get(i).copied().unwrap_or(true);
        apply_row_visual(world, *entity, selected, grayed);
    }

    ensure_selected_visible(world);
}

// --- scroll-into-view -----------------------------------------------------

/// Scroll the list so the selected row is visible, updating `ScrollPosition`.
fn ensure_selected_visible(world: &mut World) {
    let (item_entities, selected_index) = {
        let ctx = world.resource::<CommandMenuUiContext>();
        (ctx.item_entities.clone(), ctx.selected_index)
    };
    if selected_index < 0 || item_entities.is_empty() {
        return;
    }
    let sel = selected_index as usize;

    let Some(list_e) = world
        .query_filtered::<Entity, With<CommandMenuUIList>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let Some(list_cn) = world.get::<ComputedNode>(list_e) else {
        return;
    };
    let scale = list_cn.inverse_scale_factor;
    if scale <= 0.0 {
        return;
    }

    let mut sel_top = LIST_PADDING;
    let mut sel_h = 0.0_f32;
    for (i, e) in item_entities.iter().enumerate() {
        let Some(cn) = world.get::<ComputedNode>(*e) else {
            return;
        };
        let h = cn.size.y * scale;
        if i == sel {
            sel_h = h;
            break;
        }
        sel_top += h + LIST_ROW_GAP;
    }
    if sel_h <= 0.0 {
        return;
    }
    let sel_bottom = sel_top + sel_h;

    let viewport_h = (list_cn.size.y * scale - 2.0 * LIST_PADDING).max(0.0);
    if viewport_h <= 0.0 {
        return;
    }

    let cur_y = world
        .get::<ScrollPosition>(list_e)
        .map(|s| s.0.y)
        .unwrap_or(0.0);
    let new_y = if sel_top < cur_y + SCROLL_MARGIN {
        (sel_top - SCROLL_MARGIN).max(0.0)
    } else if sel_bottom > cur_y + viewport_h - SCROLL_MARGIN {
        (sel_bottom + SCROLL_MARGIN - viewport_h).max(0.0)
    } else {
        cur_y
    };

    if new_y != cur_y {
        let max_offset = (list_cn.content_size.y * scale - viewport_h).max(0.0);
        let clamped = new_y.clamp(0.0, max_offset);
        if let Some(mut sp) = world.get_mut::<ScrollPosition>(list_e) {
            sp.0.y = clamped;
        }
    }
}

// --- search filter + per-row styling --------------------------------------

/// Re-evaluate the search filter, reorder the list to put matches first,
/// snap the cursor, and re-apply per-row visuals.
fn refresh(world: &mut World) {
    let (item_entities, query) = {
        let ctx = world.resource::<CommandMenuUiContext>();
        (ctx.item_entities.clone(), ctx.query.clone())
    };
    if item_entities.is_empty() {
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        ctx.matches = Vec::new();
        ctx.selected_index = -1;
        return;
    }

    let query_lower = query.to_lowercase();
    let matches: Vec<bool> = item_entities
        .iter()
        .map(|e| {
            world
                .get::<CommandHasQueryable>(*e)
                .map(|q| query.is_empty() || q.0.to_lowercase().contains(&query_lower))
                .unwrap_or(true)
        })
        .collect();

    let mut new_order: Vec<Entity> = Vec::with_capacity(item_entities.len());
    for (i, e) in item_entities.iter().enumerate() {
        if matches[i] {
            new_order.push(*e);
        }
    }
    for (i, e) in item_entities.iter().enumerate() {
        if !matches[i] {
            new_order.push(*e);
        }
    }

    if new_order != item_entities {
        if let Some(list) = list_entity(world) {
            let mut list_mut = world.entity_mut(list);
            for (i, e) in new_order.iter().enumerate() {
                list_mut.insert_related::<ChildOf>(i, &[*e]);
            }
        }
    }

    let num_matches = matches.iter().filter(|m| **m).count();
    let new_selected_index = if num_matches == 0 { -1 } else { 0 };

    if let Some(list) = list_entity(world) {
        if let Some(mut sp) = world.get_mut::<ScrollPosition>(list) {
            sp.0 = Vec2::ZERO;
        }
    }

    {
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        ctx.item_entities = new_order.clone();
        ctx.matches = matches.clone();
        ctx.selected_index = new_selected_index;
    }

    for (i, e) in new_order.iter().enumerate() {
        let selected = (i as i32) == new_selected_index;
        let grayed = !matches.get(i).copied().unwrap_or(true);
        apply_row_visual(world, *e, selected, grayed);
    }
}

/// Apply the row's selected/grayed/normal background and name colour.
fn apply_row_visual(world: &mut World, entity: Entity, selected: bool, grayed: bool) {
    let bg = if selected {
        PALETTE_ROW_SELECTED
    } else if grayed {
        ROW_PANEL_GRAYED
    } else {
        PALETTE_ROW
    };
    if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
        background.0 = bg;
    }
    if let Some(name_text) = world.get::<RowNameText>(entity).map(|c| c.0) {
        let color = if grayed { NAME_COLOR_GRAYED } else { PALETTE_NAME };
        if let Some(mut text_color) = world.get_mut::<TextColor>(name_text) {
            text_color.0 = color;
        }
    }
}

/// Local row colours; the palette owns the grayed colour since grayed is search-only.
const PALETTE_ROW: Color = Color::srgb(0.16, 0.16, 0.20);
const PALETTE_ROW_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
const PALETTE_NAME: Color = Color::srgb(0.96, 0.96, 0.98);

/// The single `CommandMenuUIList` entity (singleton, spawned at startup).
fn list_entity(world: &mut World) -> Option<Entity> {
    world
        .query_filtered::<Entity, With<CommandMenuUIList>>()
        .iter(world)
        .next()
}

//! The command palette: panel shell + open/close + search bar. Each
//! command's UI is spawned by [`crate::commands::core::spawn_command`];
//! this module owns the panel's container, the search bar above the list,
//! the visibility + input-layer transitions, and the search filter
//! (substring match + reorder + grayed-out non-matches).

use bevy::ecs::message::MessageCursor;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::resources::input_layer::InputLayer;

// --- shell components -------------------------------------------------------

#[derive(Component)]
pub struct CommandMenuUIRoot;
/// The window that wraps the search bar + the scrollable list. Holds the
/// border / background; the search bar and the list are its children.
#[derive(Component)]
pub struct CommandMenuUIWindow;
/// The scrollable inner list. Owns the row children; `ensure_selected_visible`
/// and `refresh` write its `ScrollPosition` and `Children` membership.
#[derive(Component)]
pub struct CommandMenuUIList;

/// The search bar container (the row above the list). A `MenuSearchText`
/// child mirrors [`CommandMenuUiContext::query`] so the player sees what
/// they typed.
#[derive(Component)]
pub struct MenuSearch;
/// The text node inside the search bar. Its `Text` component is rewritten
/// every frame from [`CommandMenuUiContext::query`].
#[derive(Component)]
pub struct MenuSearchText;

// --- row metadata ----------------------------------------------------------

/// Tag stamped on every command row entity by the spawning command. The
/// value is that command's
/// [`BaseCommand::get_command_id`](crate::commands::BaseCommand::get_command_id)
/// (e.g. `"command:construct_building"`). The orchestrator's [`update`]
/// reads this and dispatches the selection visual only to the command
/// that matches, so commands no longer have to ignore entities that
/// aren't theirs.
#[derive(Component, Clone, Debug)]
pub struct CommandHasId(pub String);

/// Search key stamped on every palette row by `picker_row`. The palette's
/// search bar matches the query against this string (case-insensitive
/// substring). The stamp is the row's display name, so a row labelled
/// `"Castle (-cost)"` still matches `"castle"` under substring search
/// (the `(-cost)` suffix is included but does not break the prefix match).
/// A row that lacks this component is treated as "always matches" so a
/// future command that doesn't go through `picker_row` can't accidentally
/// hide its rows behind search.
#[derive(Component, Clone, Debug)]
pub struct CommandHasQueryable(pub String);

/// The row's name-text child entity. Stamped by `picker_row` so the
/// palette can recolour the name alone when the row is grayed (the
/// stat columns stay readable).
#[derive(Component, Clone, Debug)]
pub struct RowNameText(pub Entity);

/// Key tag on a step row (e.g. a land the player can pick from). Paired
/// with [`CommandHasValue`]; a future dispatch layer reads
/// `(CommandHasKey, CommandHasValue)` and pushes the pair into the
/// context's `choices`.
#[derive(Component, Clone, Debug)]
pub struct CommandHasKey(pub String);

/// Value tag on a step row — the concrete id the row represents (e.g.
/// a land id like `"land:riverrun"`). Paired with [`CommandHasKey`].
#[derive(Component, Clone, Debug)]
pub struct CommandHasValue(pub String);

// --- context ---------------------------------------------------------------

/// Per-open state the palette exposes to the rest of the game.
/// `item_entities` holds every entity the orchestrator just spawned
/// (each command's rows, in [`COMMANDS`](crate::commands::COMMANDS)
/// order, in the order each command produced them). After a search
/// filter the list is reordered so matches come first; the parallel
/// `matches` bit vec tracks which rows passed the filter.
/// `selected_index` is the cursor's index into `item_entities` — `0` on
/// open (the first row), `-1` when the palette has nothing rendered
/// (closed or every row filtered out by a search query).
/// `choices` is the running `(key, value)` selection list the player
/// has built up — Enter on a row pushes `("command", <id>)` here and the
/// orchestrator re-spawns against the updated list. Cleared on close.
/// `query` is the current search-bar text; cleared on every panel change
/// (top-level → step → step) and on open/close per the palette's
/// search-lifecycle rule.
/// `cursor` is the [`MessageCursor<KeyboardInput>`] the palette drains
/// to capture typed characters + Backspace. Stored on the resource so
/// the exclusive `input` system can read it without needing a
/// `MessageReader` system param.
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
const SEARCH_TEXT: Color = Color::srgb(0.96, 0.96, 0.98);
const SEARCH_PLACEHOLDER: Color = Color::srgba(0.55, 0.55, 0.62, 0.85);

/// Mirrors the row's `BackgroundColor` when the search bar has filtered
/// it out. Slightly darker than `commands::core::ROW_PANEL` so a grayed
/// row reads as "still here, but dimmed" rather than "fully disabled".
const ROW_PANEL_GRAYED: Color = Color::srgb(0.12, 0.12, 0.15);
/// Name-text color when the row is grayed. The stat columns stay readable
/// (their own colors are still meaningful — gold, levy, etc.).
const NAME_COLOR_GRAYED: Color = Color::srgba(0.96, 0.96, 0.98, 0.35);

// --- scroll math ----------------------------------------------------------

/// Mirrors `padding: UiRect::all(px(10))` on `CommandMenuUIList`. If you
/// change one, change the other — `ensure_selected_visible` uses these
/// to figure out where the content area starts.
const LIST_PADDING: f32 = 10.0;
/// Mirrors `row_gap: px(6)` on `CommandMenuUIList`.
const LIST_ROW_GAP: f32 = 6.0;
/// Logical px of breathing room kept between the selected row and the
/// viewport edge when scrolling it into view.
const SCROLL_MARGIN: f32 = 8.0;

// --- startup --------------------------------------------------------------

/// Spawn the v2 palette's panel shell once, hidden. The window is a
/// column flex holding the search bar on top and the scrollable list
/// below; the backdrop fills the screen and centres the window.
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
                // Search bar: a styled row above the list. The text child
                // carries `MenuSearchText` so `mirror_search_text` (called
                // from `input`) can find it and rewrite the displayed
                // query every frame.
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
                // Scrollable list. Padding + row_gap + overflow scroll
                // move down from the previous combined-into-one layout;
                // the border / background now live on the window.
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

/// Show the panel, call the data-side orchestrator to populate it, and
/// flip the input layer to `CommandMenu`. Called from
/// [`crate::ui::input::global_keys`] when the player presses `C`.
///
/// The orchestrator returns the entities the commands spawned; we stash
/// them in [`CommandMenuUiContext::item_entities`] so the panel (and
/// any future selection / dispatch layer) knows what's on screen. The
/// search-bar context fields (`query`, `matches`, `cursor`) reset on
/// every open so a fresh panel starts with an empty filter. `refresh`
/// applies the initial selection visual (cursor at row 0, or -1 when
/// the roster is empty) and the per-row `BackgroundColor` / name-text
/// styling.
pub fn open_command(world: &mut World) {
    show_panel(world);
    let (item_entities, executed) = crate::commands::core::spawn_command(world, &[]);
    {
        // Advance the typed-char cursor past the events that were
        // already in the stream when the player pressed C. Without
        // this, the "c" keypress that opened the palette would also be
        // re-read by `drain_typed_input` on the first input frame and
        // sneak into the search bar. `clear` sets the cursor's
        // `last_message_count` to the stream's current count, so the
        // next `read` sees nothing. The two borrows (immutable
        // `&Messages` + mutable `&mut CommandMenuUiContext`) have to
        // coexist, so they go through `resource_scope`, which holds
        // the immutable borrow across a `&mut World` closure.
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

    // If the first spawn already had enough info to act, close the panel.
    if executed {
        close_command(world);
    }
}

/// Despawn every command's spawned UI, clear the context, hide the panel,
/// and flip the input layer back to `Root`. Called from this module's
/// [`input`] on `Esc`.
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

/// Despawn every row tracked in [`CommandMenuUiContext::item_entities`].
/// No-op before any command has spawned its UI.
fn despawn_command_rows(world: &mut World) {
    let entities = world.resource::<CommandMenuUiContext>().item_entities.clone();
    for e in entities {
        world.despawn(e);
    }
}

// --- input -----------------------------------------------------------------

/// Run condition: the command-menu input layer is active (palette is open).
/// Pair with `input` via `.run_if` so it stays dormant on the root layer.
pub fn command_menu_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::CommandMenu
}

/// Per-frame input handler for the palette. Gated to the command-menu
/// layer via [`command_menu_layer_active`].
///
/// Order:
/// 1. Drain `Messages<KeyboardInput>` via the `cursor` stored on the
///    context — collect typed characters + Backspace, mutate `query`,
///    then call `refresh` exactly once at the end if the query changed.
/// 2. Mirror `context.query` into the `MenuSearchText` text node so the
///    player sees their input.
/// 3. Handle **Esc** (close), **Enter** (confirm pick), and delegate
///    arrow-key navigation to [`navigation`].
///
/// The exclusive signature is needed because `refresh` reorders
/// `ChildOf` relationships and `handle_enter` re-spawns UI; both want
/// `&mut World`.
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

/// Drain `Messages<KeyboardInput>` via the context's `MessageCursor` and
/// translate the events into `query` mutations. Two event kinds are
/// consumed:
/// - `KeyCode::Backspace` (pressed) → pop one char from `query`.
/// - `Key::Character(_)` or `Key::Space` (typed char) → append the
///   `text` field to `query`.
///
/// Detecting "typed char" via `Key::Character` (and `Key::Space`, which
/// is its own named-key variant in Bevy) is the canonical Bevy way;
/// checking the `text` field alone would also match Enter/Tab/etc. on
/// platforms where winit sets `text` on control keys (Windows sets
/// `text: Some("\r")` for Enter, which would otherwise end up in the
/// search bar and silently filter the list). The `Key` enum's named
/// variants stay disjoint from `Key::Character`, so the match is exact.
fn drain_typed_input(world: &mut World) {
    // Snapshot the cursor and gather events that affect the query. We
    // need to drop the `Messages<KeyboardInput>` borrow before mutating
    // `context.query`, so the events are collected first.
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
        // No query-affecting events — still write the cursor back so the
        // event cursor advances past events we ignored (Enter, Esc, etc.).
        world.resource_mut::<CommandMenuUiContext>().cursor = cursor;
        return;
    }
    {
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        for event in &typed {
            if event.key_code == KeyCode::Backspace {
                ctx.query.pop();
            } else if let Some(text) = event.text.as_deref() {
                // `Key::Space` produces `text: Some(" ")`, and
                // `Key::Character` produces the typed glyph (or a
                // multi-char string on some Windows dead-key sequences).
                // Both filter through this branch; control-char
                // keypresses never reach here because the `Key` match
                // above excluded them.
                for ch in text.chars() {
                    ctx.query.push(ch);
                }
            }
        }
        ctx.cursor = cursor;
    }
    refresh(world);
}

/// Mirror `CommandMenuUiContext::query` into the `MenuSearchText` text
/// node. Runs every frame, even when the query is empty, so the bar
/// always reflects the current state. With an empty query the bar shows
/// a muted placeholder; any non-empty query renders verbatim with a
/// `>` prefix.
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

/// **Enter** handler: capture the currently-selected row's metadata
/// into the context's `choices` list, then despawn every existing row
/// and re-spawn against the updated `choices`. Two row shapes:
///
/// - A command row carries [`CommandHasId`] → push
///   `("command", <id>)` unless `choices` already has a `"command"`
///   key (the pick is sticky once made).
/// - A step row carries [`CommandHasKey`] + [`CommandHasValue`] →
///   push `(key, value)` so the next spawn sees the step pick.
///
/// Each command's `spawn_command` inspects the choices and either renders
/// its row, renders the next step, or no-ops. The query clears on every
/// panel change so the next panel starts with a fresh filter.
///
/// No-op when no row is selected (`selected_index < 0`, the "all rows
/// filtered out by the search" case): the panel's rows would otherwise
/// be torn down and re-spawned against the same choices, which is a
/// visible flicker for a keypress the player can't act on.
fn handle_enter(world: &mut World) {
    if world.resource::<CommandMenuUiContext>().selected_index < 0 {
        return;
    }
    // Snapshot the selected entity.
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
        // Push the owning command's id if the row carries one. The check
        // is "command" not in `choices` yet, so re-picking the same
        // command is a no-op (the pick is sticky across re-opens).
        if let Some(id) = world.get::<CommandHasId>(entity).map(|c| c.0.clone()) {
            let mut context = world.resource_mut::<CommandMenuUiContext>();
            if !context.choices.iter().any(|(k, _)| k == "command") {
                context.choices.push(("command".to_string(), id));
            }
        }
        // Push the step pick if the row carries a key/value pair. This
        // runs independently of the command push above, so a step row
        // that *also* has `CommandHasId` (e.g. a land row, which is
        // both a child of a command and a step) records both the
        // command and the step in one Enter.
        if let (Some(key), Some(value)) = (
            world.get::<CommandHasKey>(entity).map(|c| c.0.clone()),
            world.get::<CommandHasValue>(entity).map(|c| c.0.clone()),
        ) {
            let mut context = world.resource_mut::<CommandMenuUiContext>();
            context.choices.push((key, value));
        }
    }

    // Despawn every existing row + clear the roster.
    let entities: Vec<Entity> = world
        .resource::<CommandMenuUiContext>()
        .item_entities
        .clone();
    for e in entities {
        world.despawn(e);
    }

    // Re-spawn with the updated choices. Each command decides whether to
    // render itself based on whether its id is the pick.
    let choices = world.resource::<CommandMenuUiContext>().choices.clone();
    let (new_entities, executed) = crate::commands::core::spawn_command(world, &choices);

    // Reset the query on every panel change — each panel starts with a
    // fresh filter rather than carrying the previous step's text forward.
    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.item_entities = new_entities;
        context.query.clear();
        context.matches.clear();
    }
    refresh(world);

    // If the re-spawn reported it had enough info to act (e.g. construct
    // with both land + building picks), close the panel.
    if executed {
        close_command(world);
    }
}

/// Arrow-key navigation: on **ArrowUp** / **ArrowDown**, move
/// [`CommandMenuUiContext::selected_index`] by one (wrapping at the
/// ends) and re-apply the selection visual to every row. With an empty
/// query the cursor walks the full list; with a non-empty query the
/// cursor walks only the matches (skipping grayed rows), wrapping at
/// the match-list's ends. No-op when the roster is empty or every row
/// is filtered out.
fn navigation(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    if !up && !down {
        return;
    }

    // Snapshot items + cursor + matches + query so the resource borrow
    // drops before we mutate the context and call into the per-entity
    // update.
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
        // Match-only walk. The current cursor may be -1 (no matches
        // previously) or pointing at a row that got filtered out by the
        // most recent query change — fall back to the first match.
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
        // Full-list walk (the original behaviour).
        let len = item_entities.len() as i32;
        if up {
            if current <= 0 { len - 1 } else { current - 1 }
        } else {
            if current >= len - 1 { 0 } else { current + 1 }
        }
    };

    // Write the new cursor.
    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.selected_index = new_index;
    }

    // Re-apply the selection visual to every row so the highlight
    // follows the new cursor and the grayed state is preserved.
    for (i, entity) in item_entities.iter().enumerate() {
        let selected = (i as i32) == new_index;
        let grayed = !matches.get(i).copied().unwrap_or(true);
        apply_row_visual(world, *entity, selected, grayed);
    }

    // Then scroll the newly selected row into view, if it scrolled out
    // when navigating. No-op when the row already fits or layout hasn't
    // run yet (first frame after spawn).
    ensure_selected_visible(world);
}

// --- scroll-into-view -----------------------------------------------------

/// Scroll [`CommandMenuUIList`] so the row at
/// [`CommandMenuUiContext::selected_index`] is visible, updating the
/// list's [`ScrollPosition`] when needed. Bail-out conditions:
/// - the roster is empty or no row is selected;
/// - the list's [`ComputedNode`] isn't computed yet (first frame after a
///   spawn — the next input tick retries);
/// - any row's [`ComputedNode`] is missing (layout hasn't run for it yet).
///
/// Row heights come from `ComputedNode::size().y`, multiplied by
/// `inverse_scale_factor` to land in logical pixels (matching how Bevy
/// itself measures scroll position). Padding + row_gap are added by hand
/// so the scroll-into-view math matches what the user sees on screen;
/// border thickness is ignored (1 logical px, smaller than
/// [`SCROLL_MARGIN`]).
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

    // Walk rows up to (and including) the selected one, accumulating
    // logical-px heights plus the inter-row gap. Bail if any row in the
    // prefix hasn't been laid out yet — better to skip this frame than
    // scroll against stale sizes.
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

    // Viewport = outer height (in logical px) minus top+bottom padding.
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
        // Clamp to the scrollable range. max_offset is how far content
        // can scroll past 0 before its tail hits the viewport bottom —
        // matching how Bevy's own scroll handler clamps deltas.
        let max_offset = (list_cn.content_size.y * scale - viewport_h).max(0.0);
        let clamped = new_y.clamp(0.0, max_offset);
        if let Some(mut sp) = world.get_mut::<ScrollPosition>(list_e) {
            sp.0.y = clamped;
        }
    }
}

// --- search filter + per-row styling --------------------------------------

/// Re-evaluate the search filter against the current roster and update
/// the visible state. Single entry point for every code path that can
/// change the visible row set or the query:
/// - `open_command` — fresh panel, query reset to empty.
/// - `handle_enter` — panel change, query reset to empty.
/// - `drain_typed_input` — player typed a char or hit Backspace.
///
/// Steps:
/// 1. Recompute `matches` against each row's [`CommandHasQueryable`].
/// 2. Build `[matches..., non-matches...]` and re-parent the list's
///    `Children` so the visual order matches.
/// 3. Snap the cursor: `0` when at least one row matches, `-1` when
///    nothing matches.
/// 4. Reset the list's scroll position so the player sees the top of
///    the reordered list (their cursor lands there).
/// 5. Apply per-row visuals (background + name text colour) for every
///    row.
///
/// Rows without a [`CommandHasQueryable`] are treated as "always
/// matches" so a future command that doesn't route through `picker_row`
/// never disappears behind a search.
fn refresh(world: &mut World) {
    let (item_entities, query) = {
        let ctx = world.resource::<CommandMenuUiContext>();
        (ctx.item_entities.clone(), ctx.query.clone())
    };
    if item_entities.is_empty() {
        // Nothing to filter. Keep the context matches vec empty + cursor
        // at -1 so callsites don't act on stale state.
        let matches: Vec<bool> = Vec::new();
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        ctx.matches = matches;
        ctx.selected_index = -1;
        return;
    }

    // 1. Compute matches (case-insensitive substring).
    let query_lower = query.to_lowercase();
    let matches: Vec<bool> = item_entities
        .iter()
        .map(|e| {
            world
                .get::<CommandHasQueryable>(*e)
                .map(|q| {
                    query.is_empty() || q.0.to_lowercase().contains(&query_lower)
                })
                .unwrap_or(true)
        })
        .collect();

    // 2. Build the new visual order: matches first (in their original
    //    relative order), then non-matches (in their original relative
    //    order). A row already in the right slot is skipped to avoid
    //    gratuitous `insert_related` work.
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

    // 3. Re-parent the list's children. Skipped when nothing changed.
    if new_order != item_entities {
        if let Some(list) = list_entity(world) {
            let mut list_mut = world.entity_mut(list);
            for (i, e) in new_order.iter().enumerate() {
                list_mut.insert_related::<ChildOf>(i, &[*e]);
            }
        }
    }

    // 4. Cursor: top of the matches. Nothing matches → no cursor.
    let num_matches = matches.iter().filter(|m| **m).count();
    let new_selected_index = if num_matches == 0 { -1 } else { 0 };

    // 5. Reset scroll to top so the player sees the new top of the
    //    list (the first match, or the still-grayed original ordering
    //    when nothing matches).
    if let Some(list) = list_entity(world) {
        if let Some(mut sp) = world.get_mut::<ScrollPosition>(list) {
            sp.0 = Vec2::ZERO;
        }
    }

    // 6. Write the new state to the context.
    {
        let mut ctx = world.resource_mut::<CommandMenuUiContext>();
        ctx.item_entities = new_order.clone();
        ctx.matches = matches.clone();
        ctx.selected_index = new_selected_index;
    }

    // 7. Apply per-row visuals.
    for (i, e) in new_order.iter().enumerate() {
        let selected = (i as i32) == new_selected_index;
        let grayed = !matches.get(i).copied().unwrap_or(true);
        apply_row_visual(world, *e, selected, grayed);
    }
}

/// The single place the row's visual state is written. Three states:
/// - selected: blue background, full-white name.
/// - grayed: dim background, faded name.
/// - normal: panel background, full-white name.
///
/// Selected takes precedence over grayed (a row that's both — within the
/// filtered list but the cursor is on it — still reads as selected);
/// the grayed background is a fallback for non-selected non-matches.
/// Uses `commands::core::ROW_PANEL` / `ROW_PANEL_SELECTED` indirectly
/// by re-implementing the same colour choices here so the palette
/// doesn't need a back-reference into the `commands` module.
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

/// Local copies of the unselected-row + selected-row + name colours so
/// the palette doesn't need to import from `commands::core`. Keeping
/// the values in sync is the only cross-module coupling — the palette
/// owns the grayed colour since grayed is a search-only state.
const PALETTE_ROW: Color = Color::srgb(0.16, 0.16, 0.20);
const PALETTE_ROW_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
const PALETTE_NAME: Color = Color::srgb(0.96, 0.96, 0.98);

/// First (and only) `CommandMenuUIList` entity. The list is a singleton
/// spawned once at startup, so a single archetype scan is all that's
/// needed. Returns `None` if the panel hasn't been spawned yet (the
/// startup schedule hasn't run).
fn list_entity(world: &mut World) -> Option<Entity> {
    world
        .query_filtered::<Entity, With<CommandMenuUIList>>()
        .iter(world)
        .next()
}

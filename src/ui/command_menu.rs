//! The command palette: panel shell + open/close. Each command's UI is
//! spawned by [`crate::commands::core::spawn_command`]; this module owns
//! the panel's container and the visibility + input-layer transitions.

use bevy::prelude::*;

use crate::resources::input_layer::InputLayer;

// --- shell components -------------------------------------------------------

#[derive(Component)]
pub struct CommandMenuUIRoot;
#[derive(Component)]
pub struct CommandMenuUIList;

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
/// order, in the order each command produced them).
/// `selected_index` is the cursor's index into `item_entities` — `0` on
/// open (the first row), `-1` when the palette has nothing rendered
/// (closed or an empty roster).
/// `choices` is the running `(key, value)` selection list the player
/// has built up — Enter on a row pushes `("command", <id>)` here and the
/// orchestrator re-spawns against the updated list. Cleared on close.
#[derive(Resource, Default)]
pub struct CommandMenuUiContext {
    pub item_entities: Vec<Entity>,
    pub selected_index: i32,
    pub choices: Vec<(String, String)>,
}

// --- styling ---------------------------------------------------------------

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);

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

/// Spawn the v2 palette's panel shell once, hidden.
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
                CommandMenuUIList,
                Node {
                    width: percent(45),
                    // Cap the panel at 70% of screen height; rows that
                    // spill past it get clipped and scrolled instead of
                    // running off-screen. `Overflow::scroll_y()` enables
                    // Bevy's built-in scroll container, driven by the
                    // `ScrollPosition` component on the same entity.
                    max_height: percent(70),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(LIST_PADDING)),
                    row_gap: px(LIST_ROW_GAP),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(BORDER),
                ScrollPosition::default(),
            ));
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
/// cursor (`selected_index`) starts at `0` when at least one row was
/// spawned, or `-1` when the roster came back empty. After the cursor
/// is set, the loop walks every row and calls
/// [`crate::commands::core::update`] on it — so the first row's
/// `Construct Building` card picks up its blue highlight on the very
/// first frame the panel is visible (no need to wait for a per-frame
/// tick).
pub fn open_command(world: &mut World) {
    show_panel(world);
    let (item_entities, executed) = crate::commands::core::spawn_command(world, &[]);
    let selected_index = if item_entities.is_empty() { -1 } else { 0 };

    // Apply selection visuals for every row so the panel renders correctly
    // on the first frame. The orchestrator's `update` delegates to the
    // command's own `update` (Construct Building for now) which decides
    // the actual visual change.
    for (i, entity) in item_entities.iter().enumerate() {
        let is_selected = (i as i32) == selected_index;
        crate::commands::core::update(*entity, is_selected, world);
    }

    // Then stash the roster + cursor in the context.
    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.item_entities = item_entities;
        context.selected_index = selected_index;
    }

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
/// layer via [`command_menu_layer_active`]; handles **Esc** (close on
/// release), **Enter** (confirm the selected row — see
/// [`handle_enter`]), and delegates arrow-key navigation to
/// [`navigation`].
pub fn input(world: &mut World) {
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
/// its row, renders the next step, or no-ops.
fn handle_enter(world: &mut World) {
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

    let selected_index = if new_entities.is_empty() { -1 } else { 0 };

    // Apply the selection visual to every row.
    for (i, entity) in new_entities.iter().enumerate() {
        let is_selected = (i as i32) == selected_index;
        crate::commands::core::update(*entity, is_selected, world);
    }

    // Stash the new roster + cursor in the context.
    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.item_entities = new_entities;
        context.selected_index = selected_index;
    }

    // If the re-spawn reported it had enough info to act (e.g. construct
    // with both land + building picks), close the panel.
    if executed {
        close_command(world);
    }
}

/// Arrow-key navigation: on **ArrowUp** / **ArrowDown**, move
/// [`CommandMenuUiContext::selected_index`] by one (wrapping at the
/// ends) and re-apply the selection visual to every row by calling
/// [`crate::commands::core::update`]. No-op when the roster is empty.
fn navigation(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    if !up && !down {
        return;
    }

    // Snapshot items + current cursor so the resource borrow drops before
    // we mutate the context and call into the per-entity update.
    let (item_entities, current) = {
        let context = world.resource::<CommandMenuUiContext>();
        (context.item_entities.clone(), context.selected_index)
    };
    if item_entities.is_empty() {
        return;
    }

    let len = item_entities.len() as i32;
    let new_index = if up {
        // Wrap: 0 -> last, otherwise step down.
        if current <= 0 { len - 1 } else { current - 1 }
    } else {
        // Wrap: last -> 0, otherwise step up.
        if current >= len - 1 { 0 } else { current + 1 }
    };

    // Write the new cursor.
    {
        let mut context = world.resource_mut::<CommandMenuUiContext>();
        context.selected_index = new_index;
    }

    // Re-apply the selection visual to every row so the highlight
    // follows the new cursor. The orchestrator's `update` delegates to
    // the command's own `update` (Construct Building for now).
    for (i, entity) in item_entities.iter().enumerate() {
        let is_selected = (i as i32) == new_index;
        crate::commands::core::update(*entity, is_selected, world);
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

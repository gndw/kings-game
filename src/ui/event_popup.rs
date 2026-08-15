//! The event popup: a modal that surfaces one of the events from
//! [`crate::game::event_data::EVENT_DEFS`]. Mirrors the error-popup shape
//! (backdrop + window + title + body + choices list + hint) with a vertical
//! stack of choice rows in place of the single Esc-to-close body.
//!
//! Input: `↑` / `↓` move the cursor through choices (wraps); `Enter` fires
//! [`OnEventResolved`] with `Some(cursor)`; `Esc` fires the same event with
//! `None` (forfeit). The system is gated to
//! [`InputLayer::Event`](crate::resources::input_layer::InputLayer::Event),
//! which the `OnEventPresented` observer flips to after showing the popup.

use bevy::prelude::*;

use crate::observers::{OnEventPresented, OnEventResolved};
use crate::game::event_data::EVENT_DEFS;
use crate::game::presenting_event::EventDeck;
use crate::resources::input_layer::InputLayer;

#[derive(Component)]
pub struct EventPopupUIRoot;
#[derive(Component)]
pub struct EventPopupTitle;
#[derive(Component)]
pub struct EventPopupNarration;
/// Empty column where `on_event_presented` spawns the choice rows.
#[derive(Component)]
pub struct EventPopupChoicesContainer;
/// Marker on a single choice-row.
#[derive(Component)]
pub struct EventPopupChoiceRow;
#[derive(Component)]
pub struct EventPopupHint;

/// Modal UI state — cursor + the spawned choice rows. Resets on every
/// presentation. Outside the popup this is unused.
#[derive(Resource, Default)]
pub struct EventPopupUiContext {
    pub choice_rows: Vec<Entity>,
    pub cursor: usize,
    /// Stable name cache so the chronicle observer and the popup body can
    /// resolve `{name}` without needing a fresh `world.get` each frame.
    pub pending_attendee_name: Option<String>,
}

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.10, 0.08, 0.12);
const WINDOW_BORDER: Color = Color::srgba(0.85, 0.55, 0.30, 0.65);
const TITLE_COLOR: Color = Color::srgb(0.95, 0.78, 0.45);
const NARRATION_COLOR: Color = Color::srgb(0.94, 0.92, 0.96);
const ROW_PANEL: Color = Color::srgb(0.16, 0.16, 0.20);
const ROW_PANEL_SELECTED: Color = Color::srgb(0.55, 0.42, 0.20);
const ROW_BORDER: Color = Color::srgba(0.55, 0.50, 0.55, 0.35);
const ROW_TEXT: Color = Color::srgb(0.96, 0.96, 0.98);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const FONT: f32 = 16.0;
/// Above the command palette (100) and the error popup (200).
const Z_INDEX: i32 = 300;

/// Spawn the popup shell once, hidden. Mirrors `ui::error::startup` exactly
/// so a designer reading one file can read the other.
pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            EventPopupUIRoot,
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
            GlobalZIndex(Z_INDEX),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(55),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(18)),
                    row_gap: px(12),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(WINDOW_BORDER),
            ))
            .with_children(|win| {
                win.spawn((
                    EventPopupTitle,
                    Text::new(""),
                    TextFont::from_font_size(FONT + 2.0),
                    TextColor(TITLE_COLOR),
                ));
                win.spawn((
                    EventPopupNarration,
                    Text::new(""),
                    TextFont::from_font_size(FONT),
                    TextColor(NARRATION_COLOR),
                ));
                // Container for the choice rows — observers spawn children
                // into this node so the `update` query can iterate them.
                win.spawn((
                    EventPopupChoicesContainer,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        margin: UiRect::top(px(8)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ));
                win.spawn((
                    EventPopupHint,
                    Text::new("\u{2191}/\u{2193} choose  Enter confirm  Esc dismiss"),
                    TextFont::from_font_size(FONT - 4.0),
                    TextColor(HINT_COLOR),
                ));
            });
        });
}

/// On `OnEventPresented`: read the pending event, write the title + narration
/// (substituting the attendee's name for `{name}` if any), spawn one choice
/// row per choice, show the modal, force-close any open command palette, and
/// flip the input layer.
pub fn on_event_presented(
    trigger: On<OnEventPresented>,
    mut commands: Commands,
) {
    // The trigger carries no payload; resolve from the resource.
    let _ = trigger;
    commands.queue(show_event_popup);
}

fn show_event_popup(world: &mut World) {
    // 1. Snapshot pending + the attendee name.
    let (def_id, title, narration, attendee_name, choices_text) = {
        let deck = world.resource::<EventDeck>();
        let pending = match deck.pending.as_ref() {
            Some(p) => p,
            None => return, // no pending — bail; observer shouldn't have fired
        };
        let def = &EVENT_DEFS[pending.def_index];
        let attendee_name = pending
            .attendee
            .and_then(|e| world.get::<crate::ecs::character::CharacterName>(e))
            .map(|n| n.0.clone());
        let resolved_narration = if let Some(name) = attendee_name.as_deref() {
            def.narration.replace("{name}", name)
        } else {
            def.narration.replace("{name}", "a stranger")
        };
        let choices: Vec<&'static str> =
            def.choices.iter().map(|c| c.text).collect();
        (def.id, def.title.to_string(), resolved_narration, attendee_name, choices)
    };
    let _ = def_id; // currently logged via narrative only; reserved for chronicle branch

    // 2. Close the palette if it's open (mirrors `ui::error::on_error_occurred`).
    crate::ui::command_menu::close_command(world);

    // 3. Write title + narration.
    if let Some(mut title_node) = world
        .query_filtered::<&mut Text, With<EventPopupTitle>>()
        .iter_mut(world)
        .next()
    {
        title_node.0 = title;
    }
    if let Some(mut nar_node) = world
        .query_filtered::<&mut Text, With<EventPopupNarration>>()
        .iter_mut(world)
        .next()
    {
        nar_node.0 = narration;
    }

    // 4. Despawn any leftover choice rows from a previous presentation.
    let stale_rows: Vec<Entity> = world
        .resource::<EventPopupUiContext>()
        .choice_rows
        .clone();
    for row in stale_rows {
        world.entity_mut(row).despawn();
    }

    // 5. Locate the choices container (single tagged entity, no scan).
    let Some(container) = world
        .query_filtered::<Entity, With<EventPopupChoicesContainer>>()
        .iter(world)
        .next()
    else {
        return;
    };

    // 6. Spawn a choice row per choice.
    let mut rows = Vec::new();
    for (i, text) in choices_text.iter().enumerate() {
        let row = world
            .spawn((
                EventPopupChoiceRow,
                Node {
                    width: percent(100),
                    padding: UiRect::all(px(10)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(ROW_PANEL),
                BorderColor::all(ROW_BORDER),
                ChildOf(container),
            ))
            .with_children(|c| {
                c.spawn((
                    Text::new(format!("{}. {text}", i + 1)),
                    TextFont::from_font_size(FONT),
                    TextColor(ROW_TEXT),
                ));
            })
            .id();
        rows.push(row);
    }

    // 7. Update the UI context, show the root, flip the layer.
    world.resource_mut::<EventPopupUiContext>().choice_rows = rows;
    world.resource_mut::<EventPopupUiContext>().cursor = 0;
    world.resource_mut::<EventPopupUiContext>().pending_attendee_name = attendee_name;
    if let Some(mut node) = world
        .query_filtered::<&mut Node, With<EventPopupUIRoot>>()
        .iter_mut(world)
        .next()
    {
        node.display = Display::Flex;
    }
    *world.resource_mut::<InputLayer>() = InputLayer::Event;
}

/// On `OnEventResolved`: hide the modal and flip the layer back to Root.
/// Game-logic side effects (running the effect, scheduling the next event,
/// unpausing) live in [`crate::game::presenting_event::on_event_resolved`].
pub fn on_event_resolved(
    trigger: On<OnEventResolved>,
    mut commands: Commands,
) {
    let _ = trigger;
    commands.queue(hide_event_popup);
}

fn hide_event_popup(world: &mut World) {
    if let Some(mut node) = world
        .query_filtered::<&mut Node, With<EventPopupUIRoot>>()
        .iter_mut(world)
        .next()
    {
        node.display = Display::None;
    }
    *world.resource_mut::<InputLayer>() = InputLayer::Root;
    world.resource_mut::<EventPopupUiContext>().cursor = 0;
    world.resource_mut::<EventPopupUiContext>().choice_rows.clear();
}

/// Per-frame: colour the active choice row. Text colour and prefix stay
/// constant (the background flip + the input cursor's own echo are enough
/// affordance). Runs only while the popup is the active input layer.
pub fn event_popup_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Event
}

pub fn update(
    ui_ctx: Res<EventPopupUiContext>,
    mut rows: Query<(Entity, &mut BackgroundColor), With<EventPopupChoiceRow>>,
) {
    for (row_e, mut bg) in rows.iter_mut() {
        let is_selected = ui_ctx
            .choice_rows
            .get(ui_ctx.cursor)
            .copied()
            .map(|e| e == row_e)
            .unwrap_or(false);
        bg.0 = if is_selected { ROW_PANEL_SELECTED } else { ROW_PANEL };
    }
}

/// Run condition: the event-popup input layer is active.
pub fn input_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Event
}

/// Per-frame input: `↑`/`↓` move the cursor, `Enter` resolves, `Esc` forfeits.
/// Mutates the cursor directly via `EventPopupUiContext`; fires the resolve
/// event with the captured cursor (so the input system doesn't need `&mut
/// World` for the trigger).
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    deck: Res<EventDeck>,
    mut ui_ctx: ResMut<EventPopupUiContext>,
    mut commands: Commands,
) {
    // No pending event → nothing to do.
    let Some(pending) = deck.pending.as_ref() else {
        return;
    };
    let def = &EVENT_DEFS[pending.def_index];
    let n = def.choices.len();
    if n == 0 {
        return;
    }

    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    let enter = keys.just_pressed(KeyCode::Enter);
    let esc = keys.just_pressed(KeyCode::Escape);

    if up {
        ui_ctx.cursor = if ui_ctx.cursor == 0 {
            n - 1
        } else {
            ui_ctx.cursor - 1
        };
        return;
    }
    if down {
        ui_ctx.cursor = (ui_ctx.cursor + 1) % n;
        return;
    }
    if enter {
        let pick = ui_ctx.cursor;
        commands.queue(move |world: &mut World| {
            world.trigger(OnEventResolved { choice: Some(pick) });
        });
        return;
    }
    if esc {
        commands.queue(|world: &mut World| {
            // Forfeit: same event, no choice. The resolver skips the effect,
            // clears pending, schedules next; the UI observer hides the popup.
            world.trigger(OnEventResolved { choice: None });
        });
    }
}

// ponytail: the `update` system above tweaks text via a stale-cursor trick.
// Once the third event arrives with a long choice text and a cursor that
// sticks while typing, lift the row text into its own `EventPopupChoiceText`
// component (one node per row, mutated directly) instead of querying
// children — saves a string search per row per frame.

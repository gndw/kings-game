//! The error popup: a modal that surfaces a single validation
//! rejection from a command. Commands reach for
//! [`crate::commands::core::error`] (which fires [`OnErrorOccured`])
//! when their `validate` returns `Err`; this module owns the popup
//! shell, the observer that shows it, and the input handler that
//! dismisses it on **Esc**.
//!
//! Lifecycle:
//!
//! - `startup` spawns the popup shell once, hidden (`Display::None`),
//!   sitting above the command palette in z-order.
//! - The [`on_error_occured`] observer fires on every
//!   [`OnErrorOccured`] trigger. It force-closes any open command
//!   palette (so the player isn't left with a stale UI behind the
//!   popup), updates the message, shows the popup, and flips the
//!   input layer to [`InputLayer::ErrorPopup`]. Multiple errors in
//!   quick succession just overwrite the message.
//! - [`input`] is gated to the error-popup layer and listens for
//!   **Esc** — it hides the popup and flips the layer back to
//!   [`InputLayer::Root`]. The root-layer systems then take over
//!   normally.

use crate::events::OnErrorOccured;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

// --- shell components -------------------------------------------------------

#[derive(Component)]
pub struct ErrorPopupUIRoot;

/// Marker on the message text node — the observer reads this to write
/// the latest error message into the popup body. Single entity, so a
/// `Single<&mut Text, With<ErrorPopupMessage>>` works on the observer
/// side.
#[derive(Component)]
pub struct ErrorPopupMessage;

// --- styling ---------------------------------------------------------------

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.16, 0.10, 0.10);
const BORDER: Color = Color::srgba(0.85, 0.40, 0.40, 0.65);
const TITLE_COLOR: Color = Color::srgb(0.95, 0.45, 0.45);
const BODY_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const FONT: f32 = 16.0;

/// z-index sitting above the command palette (`GlobalZIndex(100)`),
/// so the popup lands on top when an error fires while the palette
/// is open.
const Z_INDEX: i32 = 200;

// --- startup --------------------------------------------------------------

/// Spawn the popup shell once, hidden. Mirrors
/// [`crate::ui::command_menu::startup`] — the body is one column with
/// a title, a message slot, and an "Esc to close" hint; the body
/// wraps inside the centered window, which wraps inside the
/// full-screen backdrop. `Display::None` keeps it off the layout
/// until the observer flips it on.
pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            ErrorPopupUIRoot,
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
                    width: percent(50),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(16)),
                    row_gap: px(10),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(BORDER),
            ))
            .with_children(|win| {
                win.spawn((
                    Text::new("ERROR"),
                    TextFont::from_font_size(FONT),
                    TextColor(TITLE_COLOR),
                ));
                win.spawn((
                    ErrorPopupMessage,
                    Text::new(""),
                    TextFont::from_font_size(FONT),
                    TextColor(BODY_COLOR),
                ));
                win.spawn((
                    Text::new("Esc to close"),
                    TextFont::from_font_size(FONT - 4.0),
                    TextColor(HINT_COLOR),
                ));
            });
        });
}

// --- observer -------------------------------------------------------------

/// Observer for [`OnErrorOccured`]. Shows the popup, writes
/// `event.message` into the body, force-closes any open command
/// palette (so the player isn't left with a stale palette UI behind
/// the popup), and flips the input layer to
/// [`InputLayer::ErrorPopup`]. If the popup was already up, only the
/// message is refreshed — the player keeps reading the latest error.
///
/// The force-close of the palette is a defensive UX choice: an error
/// from `validate` interrupts the player's command flow, and the
/// cleanest landing is a single dismissable modal over a normal Root
/// state, not a modal over a palette that won't accept input anymore.
/// The palette's existing `close_command` does the cleanup (despawn
/// rows, clear context, hide panel, set layer to Root) — we set the
/// layer to `ErrorPopup` right after so the popup owns input.
///
/// Bevy 0.19 forbids `&mut World` in observers ("Exclusive system
/// may not be used as observer"), and the cleanup mixes structural
/// changes (`close_command` despawns palette rows) with non-
/// structural ones (`Node.display`, `Text.0`, `InputLayer`).
/// `commands.queue` accepts a `FnOnce(&mut World)` closure (the
/// `Command` trait is blanket-impl'd for such closures) and applies
/// it when Bevy flushes the observer's command queue — despawn →
/// show popup → write message → flip layer to `ErrorPopup`. All
/// four writes land in one pass so the palette is torn down before
/// the popup appears, and the layer is `ErrorPopup` before the next
/// frame's input system runs.
pub fn on_error_occured(trigger: On<OnErrorOccured>, mut commands: Commands) {
    let message = trigger.event().message.clone();
    commands.queue(move |world: &mut World| {
        crate::ui::command_menu::close_command(world);
        let root_e = world
            .query_filtered::<Entity, With<ErrorPopupUIRoot>>()
            .iter(world)
            .next();
        if let Some(root_e) = root_e
            && let Some(mut node) = world.get_mut::<Node>(root_e)
        {
            node.display = Display::Flex;
        }
        let body_e = world
            .query_filtered::<Entity, With<ErrorPopupMessage>>()
            .iter(world)
            .next();
        if let Some(body_e) = body_e
            && let Some(mut text) = world.get_mut::<Text>(body_e)
        {
            text.0 = message;
        }
        *world.resource_mut::<InputLayer>() = InputLayer::ErrorPopup;
    });
}

// --- input -----------------------------------------------------------------

/// Run condition: the error-popup input layer is active (popup is up).
/// Pair with [`input`] via `.run_if` so it stays dormant on root or
/// while the command palette owns input.
pub fn error_popup_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::ErrorPopup
}

/// Per-frame input handler for the popup. Gated to the error-popup
/// layer via [`error_popup_layer_active`]; listens for **Esc**
/// (just-released so a held key doesn't dismiss N times) and on press
/// hides the popup + flips the layer back to [`InputLayer::Root`].
/// The popup takes every other keystroke too — the same ownership
/// model the command palette uses.
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut layer: ResMut<InputLayer>,
    mut root_node: Single<&mut Node, With<ErrorPopupUIRoot>>,
) {
    if !keys.just_released(KeyCode::Escape) {
        return;
    }
    root_node.display = Display::None;
    *layer = InputLayer::Root;
}

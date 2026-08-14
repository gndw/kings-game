//! The error popup: a modal that surfaces a single validation rejection from
//! a command. Commands reach for `commands::core::error` (which fires
//! `OnErrorOccured`) when their `validate` returns `Err`; this module owns
//! the popup shell, the observer that shows it, and the input handler that
//! dismisses it on `Esc`.

use crate::events::OnErrorOccured;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

#[derive(Component)]
pub struct ErrorPopupUIRoot;
/// Marker on the message text node — the observer reads this to write the
/// latest error message into the popup body.
#[derive(Component)]
pub struct ErrorPopupMessage;

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.16, 0.10, 0.10);
const BORDER: Color = Color::srgba(0.85, 0.40, 0.40, 0.65);
const TITLE_COLOR: Color = Color::srgb(0.95, 0.45, 0.45);
const BODY_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const FONT: f32 = 16.0;
/// Above the command palette's `GlobalZIndex(100)`, so the popup lands on top.
const Z_INDEX: i32 = 200;

/// Spawn the popup shell once, hidden.
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

/// Observer for `OnErrorOccured`. Shows the popup, writes the message,
/// force-closes any open command palette, and flips the input layer.
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

/// Run condition: the error-popup input layer is active.
pub fn error_popup_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::ErrorPopup
}

/// Per-frame input: `Esc` (just-released) hides the popup and restores the layer.
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

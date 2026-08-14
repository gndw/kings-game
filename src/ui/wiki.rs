//! The wiki window: a modal the player opens with **W** and closes with **Esc**.
//!
//! Style mirrors the command palette: full-screen backdrop, centered window,
//! `GlobalZIndex` above the panels. A new `InputLayer::Wiki` gates root-layer
//! keys while it's open.

use super::FONT;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

#[derive(Component)]
pub struct WikiUiRoot;

#[derive(Component)]
pub struct WikiBody;

#[derive(Resource, Default)]
pub struct WikiUiContext {
    pub open: bool,
}

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const TITLE_COLOR: Color = Color::srgb(0.96, 0.96, 0.96);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const BODY_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
const Z_INDEX: i32 = 100;

pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            WikiUiRoot,
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
                    width: percent(70),
                    height: percent(80),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(12)),
                    row_gap: px(8),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(BORDER),
            ))
            .with_children(|win| {
                win.spawn((
                    Text::new("WIKI"),
                    TextFont::from_font_size(FONT + 4.0),
                    TextColor(TITLE_COLOR),
                ));
                win.spawn((
                    Text::new("W: open / close    Esc: close"),
                    TextFont::from_font_size(FONT - 4.0),
                    TextColor(HINT_COLOR),
                ));
                win.spawn((
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::clip(),
                        padding: UiRect::all(px(6)),
                        row_gap: px(4),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.06, 0.06, 0.08)),
                ))
                .with_children(|body| {
                    body.spawn((
                        WikiBody,
                        Text::new(""),
                        TextFont::from_font_size(FONT),
                        TextColor(BODY_COLOR),
                    ));
                });
            });
        });
}

pub fn toggle_wiki(world: &mut World) {
    let layer = *world.resource::<InputLayer>();
    if layer == InputLayer::Wiki {
        close_wiki(world);
    } else {
        open_wiki(world);
    }
}

pub fn open_wiki(world: &mut World) {
    show_panel(world);
    world.resource_mut::<WikiUiContext>().open = true;
    *world.resource_mut::<InputLayer>() = InputLayer::Wiki;
}

pub fn close_wiki(world: &mut World) {
    hide_panel(world);
    world.resource_mut::<WikiUiContext>().open = false;
    *world.resource_mut::<InputLayer>() = InputLayer::Root;
}

fn show_panel(world: &mut World) {
    let Some(root) = world
        .query_filtered::<Entity, With<WikiUiRoot>>()
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
        .query_filtered::<Entity, With<WikiUiRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = Display::None;
    }
}

pub fn wiki_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Wiki
}

/// Esc closes the panel. `just_released` matches `command_menu` and
/// `error` so all popups agree on what a "close" keystroke is.
pub fn input(world: &mut World) {
    if world
        .resource::<ButtonInput<KeyCode>>()
        .just_released(KeyCode::Escape)
    {
        close_wiki(world);
    }
}

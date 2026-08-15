//! The wiki window: a modal the player opens with **W** and closes with **Esc**.
//!
//! Style mirrors the command palette: full-screen backdrop, centered window,
//! `GlobalZIndex` above the panels. A new `InputLayer::Wiki` gates root-layer
//! keys while it's open.

use super::FONT;
use crate::ecs::character::{CharacterName, CharacterOfHouse};
use crate::ecs::house::HouseName;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;

#[derive(Component)]
pub struct WikiUiRoot;

#[derive(Component)]
struct WikiHousesRoot;

#[derive(Component)]
struct WikiHousesList(pub Entity);

#[derive(Component)]
pub struct WikiBody;

#[derive(Resource, Default)]
pub struct WikiUiContext {
    pub houses_expanded: bool,
    pub house_entities: Vec<Entity>,
    pub selected_house: Option<Entity>,
}

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const NAV_BACKGROUND: Color = Color::srgb(0.06, 0.06, 0.08);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const TITLE_COLOR: Color = Color::srgb(0.96, 0.96, 0.96);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const BODY_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
const NODE_COLOR: Color = Color::srgb(0.12, 0.12, 0.16);
const NODE_SELECTED: Color = Color::srgb(0.24, 0.40, 0.72);
const NODE_INDENT: f32 = 24.0;
const Z_INDEX: i32 = 100;

pub fn startup(
    mut commands: Commands,
    house_query: Query<(Entity, &HouseName)>,
    mut context: ResMut<WikiUiContext>,
) {
    let house_entities: Vec<_> = house_query
        .iter()
        .map(|(entity, name)| (entity, name.0.clone()))
        .collect();
    context.house_entities = house_entities.iter().map(|(entity, _)| *entity).collect();
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
                    Text::new(
                        "W: toggle    Esc: close    Up/Down select    Right expand    Left collapse",
                    ),
                    TextFont::from_font_size(FONT - 4.0),
                    TextColor(HINT_COLOR),
                ));

                win.spawn((
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Row,
                        overflow: Overflow::clip(),
                        padding: UiRect::all(px(6)),
                        column_gap: px(6),
                        ..default()
                    },
                ))
                .with_children(|body| {
                    body.spawn((
                        Node {
                            width: percent(30),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(4)),
                            row_gap: px(2),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BackgroundColor(NAV_BACKGROUND),
                    ))
                    .with_children(|navigation| {
                        navigation
                            .spawn((
                                WikiHousesRoot,
                                Node {
                                    width: percent(100),
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::axes(px(8), px(6)),
                                    row_gap: px(2),
                                    border_radius: BorderRadius::all(px(4)),
                                    ..default()
                                },
                                BackgroundColor(NODE_SELECTED),
                                Text::new("> Houses"),
                                TextFont::from_font_size(FONT),
                                TextColor(BODY_COLOR),
                            ));
                        for (entity, name) in &house_entities {
                            navigation.spawn((
                                WikiHousesList(*entity),
                                Node {
                                    width: percent(100),
                                    margin: UiRect {
                                        left: px(NODE_INDENT),
                                        ..default()
                                    },
                                    padding: UiRect::axes(px(8), px(6)),
                                    border_radius: BorderRadius::all(px(4)),
                                    ..default()
                                },
                                BackgroundColor(NODE_COLOR),
                                Text::new(name.as_str()),
                                TextFont::from_font_size(FONT),
                                TextColor(BODY_COLOR),
                            ));
                        }
                    });

                    body.spawn((
                        Node {
                            width: percent(70),
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        BackgroundColor(NAV_BACKGROUND),
                    ))
                    .with_children(|body| {
                        body.spawn((
                            WikiBody,
                            Node { width: percent(100), ..default() },
                            Text::new("Select a house."),
                            TextFont::from_font_size(FONT),
                            TextColor(BODY_COLOR),
                        ));
                    });
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
    *world.resource_mut::<InputLayer>() = InputLayer::Wiki;
}

pub fn close_wiki(world: &mut World) {
    hide_panel(world);
    *world.resource_mut::<InputLayer>() = InputLayer::Root;
}

fn show_panel(world: &mut World) {
    set_house_list_visibility(world, Display::Flex);
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
    if let Some(mut text) = world.get_mut::<Text>(root) {
        text.0 = "v Houses".to_string();
    }
    world.resource_mut::<WikiUiContext>().houses_expanded = true;
}

fn hide_panel(world: &mut World) {
    set_house_list_visibility(world, Display::None);
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

fn set_house_list_visibility(world: &mut World, display: Display) {
    let house_nodes: Vec<_> = world
        .query_filtered::<Entity, With<WikiHousesList>>()
        .iter(world)
        .collect();
    for entity in house_nodes {
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.display = display;
        }
    }
}

pub fn wiki_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Wiki
}

/// Esc closes the panel. The arrow keys move through the visible navigation
/// tree, with left/right expanding and collapsing the Houses node.
pub fn input(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    let right = keys.just_pressed(KeyCode::ArrowRight);
    let left = keys.just_pressed(KeyCode::ArrowLeft);

    if keys.just_released(KeyCode::Escape) {
        close_wiki(world);
    } else if right {
        expand_houses(world);
    } else if left {
        collapse_houses(world);
    } else if up || down {
        navigate(world, up);
    }
}

fn expand_houses(world: &mut World) {
    let house_entities = world.resource::<WikiUiContext>().house_entities.clone();
    if house_entities.is_empty() || world.resource::<WikiUiContext>().houses_expanded {
        return;
    }
    set_houses_expanded(world, true);
    select_house(world, Some(house_entities[0]));
}

fn collapse_houses(world: &mut World) {
    if !world.resource::<WikiUiContext>().houses_expanded {
        return;
    }
    select_house(world, None);
    set_houses_expanded(world, false);
}

fn navigate(world: &mut World, up: bool) {
    let (houses_expanded, house_entities, selected_house) = {
        let context = world.resource::<WikiUiContext>();
        (
            context.houses_expanded,
            context.house_entities.clone(),
            context.selected_house,
        )
    };
    if !houses_expanded || house_entities.is_empty() {
        return;
    }

    let selected =
        match selected_house.and_then(|house| house_entities.iter().position(|&e| e == house)) {
            Some(index) => {
                if up {
                    index.checked_sub(1).map(|i| house_entities[i])
                } else if index + 1 < house_entities.len() {
                    Some(house_entities[index + 1])
                } else {
                    None
                }
            }
            None if up => house_entities.last().copied(),
            None => Some(house_entities[0]),
        };
    select_house(world, selected);
}

fn set_houses_expanded(world: &mut World, expanded: bool) {
    let arrow = if expanded { "v Houses" } else { "> Houses" };
    let Some(root) = world
        .query_filtered::<Entity, With<WikiHousesRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    set_house_list_visibility(
        world,
        if expanded { Display::Flex } else { Display::None },
    );
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = if expanded { Display::Flex } else { Display::None };
    }
    if let Some(mut text) = world.get_mut::<Text>(root) {
        text.0 = arrow.to_string();
    }
    world.resource_mut::<WikiUiContext>().houses_expanded = expanded;
}

fn select_house(world: &mut World, selected: Option<Entity>) {
    world.resource_mut::<WikiUiContext>().selected_house = selected;

    let Some(root) = world
        .query_filtered::<Entity, With<WikiHousesRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut background) = world.get_mut::<BackgroundColor>(root) {
        background.0 = if selected.is_none() {
            NODE_SELECTED
        } else {
            NODE_COLOR
        };
    }
    let house_nodes: Vec<_> = world
        .query::<(&WikiHousesList, Entity)>()
        .iter(world)
        .map(|(node, entity)| (node.0, entity))
        .collect();
    for (house, entity) in house_nodes {
        if let Some(mut background) = world.get_mut::<BackgroundColor>(entity) {
            background.0 = if selected == Some(house) {
                NODE_SELECTED
            } else {
                NODE_COLOR
            };
        }
    }

    let body = match selected {
        Some(house) => {
            let house_name = world
                .get::<HouseName>(house)
                .map(|name| name.0.clone())
                .unwrap_or_else(|| "Unknown house".to_string());
            let members: Vec<_> = world
                .query::<(&CharacterName, &CharacterOfHouse)>()
                .iter(world)
                .filter_map(|(name, member_of)| (member_of.0 == house).then_some(name.0.clone()))
                .collect();
            let members = if members.is_empty() {
                "- None".to_string()
            } else {
                members
                    .iter()
                    .map(|name| format!("- {name}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!("{house_name}\n\nMembers\n{members}")
        }
        None => "Select a house.".to_string(),
    };
    if let Some(body_entity) = world
        .query_filtered::<Entity, With<WikiBody>>()
        .iter(world)
        .next()
    {
        if let Some(mut text) = world.get_mut::<Text>(body_entity) {
            text.0 = body;
        }
    }
}

//! The command palette: a spotlight-style modal that launches player commands.
//!
//! Press **C** to open (or **B**/**D** from the legend to jump straight into
//! the selected land's construct/destroy step). The top-level list shows every registered
//! [`Command`](crate::commands::Command); up/down moves, **Enter** drills into
//! the picked command's own selection steps, and the final step's pick runs its
//! effect. **Escape** closes. While open it captures the arrows (so the map
//! selection doesn't move) and Escape (so the game doesn't quit) — both gated
//! by reading [`CommandMenu::open`] from `app::input` and `ui::map::update_input`.
//!
//! The palette is command-agnostic: it drives *any* registered command's steps
//! the same way, so adding a command needs no change here. [`input`] computes
//! the on-screen item list (each command's steps read the world, so it needs
//! `&World`, which only an exclusive system gets) and stores it on
//! [`CommandMenu`]; [`update`] just renders that stored list, rebuilding rows
//! only when the step/cursor moves.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::commands::{Choice, CommandRegistry, MenuItem};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;

/// The palette's state. Only [`CommandMenu::open`] is read outside this module
/// (by `app::input` and `ui::map::update_input`, to yield `esc`/arrows). The
/// rest is driven generically off the [`CommandRegistry`].
#[derive(Resource)]
pub struct CommandMenu {
    pub open: bool,
    /// The command being navigated (index into the registry), or `None` at the
    /// top-level command list.
    command: Option<usize>,
    /// The current step within the active command.
    step: usize,
    /// The cursor row.
    index: usize,
    /// The item picked at each completed step.
    choices: Vec<Choice>,
    /// The on-screen list for the current `(command, step)`, recomputed by
    /// [`input`] whenever the player moves. Stored here so the non-exclusive
    /// [`update`] can render it without `&World`.
    items: Vec<MenuItem>,
    /// The window title for the current `(command, step)`, same lifecycle.
    title: String,
}

impl Default for CommandMenu {
    fn default() -> Self {
        CommandMenu {
            open: false,
            command: None,
            step: 0,
            index: 0,
            choices: Vec::new(),
            items: Vec::new(),
            title: String::new(),
        }
    }
}

#[derive(Component)]
pub struct MenuRoot;
#[derive(Component)]
pub struct MenuTitle;
#[derive(Component)]
pub struct MenuList;

// --- palette look ----------------------------------------------------------
const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const SELECTED: Color = Color::srgb(0.24, 0.54, 0.93);
const ITEM: Color = Color::srgb(0.82, 0.82, 0.85);
const HINT: Color = Color::srgba(0.6, 0.6, 0.6, 0.8);

/// Spawn the modal hidden: a full-screen backdrop with a centered window.
pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            MenuRoot,
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
            // Cross-hierarchy ordering: the modal is its own top-level node, so
            // ZIndex (siblings only) wouldn't lift it above the panel tree.
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(45),
                    max_height: percent(70),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(10)),
                    row_gap: px(6),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(WINDOW),
                BorderColor::all(BORDER),
            ))
            .with_children(|win| {
                win.spawn((
                    MenuTitle,
                    Text::new(""),
                    TextFont::from_font_size(FONT),
                    TextColor(TITLE),
                ));
                win.spawn((
                    MenuList,
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        // ponytail: clips long rosters instead of scrolling.
                        // The base game's rosters are small; add scroll if mods grow them.
                        overflow: Overflow::clip(),
                        ..default()
                    },
                ));
                win.spawn((
                    Text::new("arrows navigate   enter select   esc close"),
                    TextFont::from_font_size(FONT),
                    TextColor(HINT),
                ));
            });
        });
}

/// One list row: full-width, highlighted when `selected`.
fn item(c: &mut ChildSpawnerCommands, label: &str, selected: bool) {
    let (bg, fg, prefix) = if selected {
        (SELECTED, Color::WHITE, "-  ")
    } else {
        (Color::NONE, ITEM, "   ")
    };
    c.spawn((
        Text::new(format!("{prefix}{label}")),
        TextFont::from_font_size(FONT),
        TextColor(fg),
        BackgroundColor(bg),
        Node {
            width: percent(100),
            padding: UiRect::all(px(3)),
            ..default()
        },
    ));
}

/// Render the stored list: toggle the overlay and rebuild the rows only when
/// `(command, step, index)` moves (the legend's table cache idea). Reads the
/// list [`input`] stored on the resource, not the world.
pub fn update(
    menu: Res<CommandMenu>,
    mut root: Single<&mut Node, With<MenuRoot>>,
    mut title: Single<&mut Text, With<MenuTitle>>,
    list: Single<Entity, With<MenuList>>,
    mut commands: Commands,
    mut cache: Local<Option<(Option<usize>, usize, usize)>>,
) {
    root.display = if menu.open { Display::Flex } else { Display::None };
    if !menu.open {
        *cache = None;
        return;
    }
    let key = (menu.command, menu.step, menu.index);
    if *cache == Some(key) {
        return;
    }
    *cache = Some(key);

    title.0 = menu.title.clone();
    let items = &menu.items;
    let index = menu.index;
    commands.entity(*list).despawn_children().with_children(|c| {
        // ponytail: no step-back navigation. An empty step is a dead end until
        // the player escapes; add Back/Bksp if commands grow multi-step paths.
        if items.is_empty() {
            item(c, "(nothing to choose)", false);
        } else {
            for (i, mi) in items.iter().enumerate() {
                item(c, &mi.label, i == index);
            }
        }
    });
}

/// Exclusive: open on **C** (or jump straight into a land command via the
/// legend's **B**/**D** hotkeys), navigate, dispatch on the final **Enter**.
/// Exclusive because it computes the on-screen list via `&World` (each command
/// decides its own steps/queries) and the last step calls the command's
/// `execute`, an `&mut World` method.
pub fn input(world: &mut World) {
    let (toggle, up, down, enter, escape, build, destroy) = {
        let keys = world.resource::<ButtonInput<KeyCode>>();
        (
            keys.just_pressed(KeyCode::KeyC),
            keys.just_pressed(KeyCode::ArrowUp),
            keys.just_pressed(KeyCode::ArrowDown),
            keys.just_pressed(KeyCode::Enter),
            keys.just_pressed(KeyCode::Escape),
            keys.just_pressed(KeyCode::KeyB),
            keys.just_pressed(KeyCode::KeyD),
        )
    };

    if !world.resource::<CommandMenu>().open {
        if toggle {
            open_menu(world, None, 0, Vec::new());
        } else if build {
            open_land_action(world, true);
        } else if destroy {
            open_land_action(world, false);
        }
        return;
    }

    if escape {
        close(world);
        return;
    }

    if up || down {
        let len = current_items(world).len();
        if len > 0 {
            let idx = world.resource::<CommandMenu>().index;
            let next = if up {
                (idx + len - 1) % len
            } else {
                (idx + 1) % len
            };
            world.resource_mut::<CommandMenu>().index = next;
        }
        refresh(world);
        return;
    }

    if enter {
        pick(world);
        // `pick` closes the menu on the final step; only refresh if it stayed open.
        if world.resource::<CommandMenu>().open {
            refresh(world);
        }
    }
}

/// Record the picked item and either advance to the next step or, on the last
/// step, hand the accumulated choices to the command's `execute`.
fn pick(world: &mut World) {
    let command = world.resource::<CommandMenu>().command;

    // Top-level: choose a command, drop into its first step.
    if command.is_none() {
        let idx = world.resource::<CommandMenu>().index;
        let count = world.resource::<CommandRegistry>().commands.len();
        if idx < count {
            let mut m = world.resource_mut::<CommandMenu>();
            m.command = Some(idx);
            m.step = 0;
            m.index = 0;
            m.choices.clear();
        }
        return;
    }

    // Within a command: record the picked item.
    let item = {
        let items = current_items(world);
        let idx = world.resource::<CommandMenu>().index;
        items.into_iter().nth(idx)
    };
    let Some(item) = item else {
        return;
    };

    let ci = command.unwrap();
    let step_count = world.resource::<CommandRegistry>().commands[ci].step_count();
    let step = world.resource::<CommandMenu>().step;
    world.resource_mut::<CommandMenu>().choices.push(Choice {
        label: item.label,
        value: item.value,
    });

    if step + 1 < step_count {
        let mut m = world.resource_mut::<CommandMenu>();
        m.step += 1;
        m.index = 0;
    } else {
        // Final step: hand the choices to the command. Clone the `Arc` first so
        // the registry's borrow is dropped before `execute` takes `&mut World`.
        let cmd = world.resource::<CommandRegistry>().commands[ci].clone();
        let actor = world.resource::<Game>().ctx.player_character_id.clone();
        let choices = world.resource::<CommandMenu>().choices.clone();
        close(world);
        cmd.execute(&choices, &actor, world);
    }
}

/// Recompute the on-screen list + title for the current `(command, step)` and
/// clamp the cursor into range.
fn refresh(world: &mut World) {
    let items = current_items(world);
    let len = items.len();
    let title = current_title(world);
    let mut m = world.resource_mut::<CommandMenu>();
    m.items = items;
    m.title = title;
    if len > 0 && m.index >= len {
        m.index = len - 1;
    }
}

/// The selectable items on screen now: the top-level command names, or the
/// active command's current step.
fn current_items(world: &World) -> Vec<MenuItem> {
    let (command, step) = {
        let m = world.resource::<CommandMenu>();
        (m.command, m.step)
    };
    let choices = world.resource::<CommandMenu>().choices.clone();
    let actor = world.resource::<Game>().ctx.player_character_id.clone();
    let registry = world.resource::<CommandRegistry>();
    match command {
        None => registry
            .commands
            .iter()
            .map(|c| MenuItem {
                label: c.name().to_string(),
                value: String::new(),
            })
            .collect(),
        Some(i) => match registry.commands.get(i) {
            Some(cmd) => cmd.step_items(step, &choices, &actor, world),
            None => Vec::new(),
        },
    }
}

/// The window title for the current `(command, step)`.
fn current_title(world: &World) -> String {
    let m = world.resource::<CommandMenu>();
    match m.command {
        None => "Command".to_string(),
        Some(i) => world
            .resource::<CommandRegistry>()
            .commands
            .get(i)
            .map(|c| c.step_title(m.step).to_string())
            .unwrap_or_else(|| "Command".to_string()),
    }
}

fn close(world: &mut World) {
    let mut m = world.resource_mut::<CommandMenu>();
    m.open = false;
    m.command = None;
    m.step = 0;
    m.index = 0;
    m.choices.clear();
}

/// Open the palette into `command` at `step` with `choices` already made. Used
/// by the **C** top-level open (`command = None`, step 0) and the legend's
/// **B**/**D** hotkeys (`command = Some(i)`, step 1, land pre-picked).
fn open_menu(world: &mut World, command: Option<usize>, step: usize, choices: Vec<Choice>) {
    {
        let mut m = world.resource_mut::<CommandMenu>();
        m.open = true;
        m.command = command;
        m.step = step;
        m.index = 0;
        m.choices = choices;
    }
    refresh(world);
}

/// Find a command in the registry by its display name. ponytail: coupling the
/// hotkey to the display name is fine while names are the canonical id; add a
/// stable command id if a mod ships a renamed variant of a base command.
fn find_command(world: &World, name: &str) -> Option<usize> {
    world
        .resource::<CommandRegistry>()
        .commands
        .iter()
        .position(|c| c.name() == name)
}

/// Open a land command (Construct/Destroy Building) straight to its building
/// step with the selected land pre-picked, skipping the command list and the
/// land step — the legend's **B**/**D** hotkeys. Fires only when the player
/// rules the selected land; otherwise the menu stays closed.
fn open_land_action(world: &mut World, construct: bool) {
    let name = if construct {
        "Construct Building"
    } else {
        "Destroy Building"
    };
    let Some(ci) = find_command(world, name) else {
        return;
    };
    let (actor, land_id) = {
        let game = world.resource::<Game>();
        (
            game.ctx.player_character_id.clone(),
            game.ctx.selected_land_id.clone(),
        )
    };
    let Some(land_id) = land_id else {
        return;
    };
    if !crate::commands::rules_land(world, &actor, &land_id) {
        return;
    }
    open_menu(
        world,
        Some(ci),
        1,
        vec![Choice {
            label: land_id.clone(),
            value: land_id,
        }],
    );
}

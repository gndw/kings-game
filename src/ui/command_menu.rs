//! The command palette: a spotlight-style modal that launches player commands.
//!
//! Press **C** to open. The top-level list shows every registered
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
//! only when `(command, step, cursor, query)` moves.
//!
//! A search bar at the top filters the list as the player types: matching
//! items move to the top, non-matches stay in the list but render in a
//! dimmer colour. The search overlay is owned entirely by the palette — no
//! command knows about it.

use super::{FONT, TITLE};
use crate::app::Game;
use crate::commands::{Choice, CommandRegistry, MenuItem};
use bevy::ecs::message::MessageCursor;
use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::prelude::*;

/// The palette's state. Only [`CommandMenu::open`] is read outside this module
/// (by `app::input` and `ui::map::update_input`, to yield `esc`/arrows). The
/// rest is driven generically off the [`CommandRegistry`].
#[derive(Resource, Default)]
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
    /// The on-screen list for the current `(command, step, query)`, recomputed
    /// by [`input`] whenever the player moves or types. Stored here so the
    /// non-exclusive [`update`] can render it without `&World`. Matches come
    /// first, then non-matches (see [`matches`](Self::matches)).
    items: Vec<MenuItem>,
    /// Parallel to `items`: `true` if the item at this index matches the
    /// current search query. An empty query means every item matches.
    matches: Vec<bool>,
    /// The window title for the current `(command, step)`, same lifecycle.
    title: String,
    /// The current search-bar text. Persists across step navigation so the
    /// player can drill into a command and keep their filter; reset on open
    /// and close.
    query: String,
    /// Tracks which `KeyboardInput` messages we've already consumed, so the
    /// exclusive `input` can drain the `Messages<KeyboardInput>` buffer
    /// without re-processing old text on subsequent frames.
    kb_cursor: MessageCursor<KeyboardInput>,
}

#[derive(Component)]
pub struct MenuRoot;
#[derive(Component)]
pub struct MenuTitle;
#[derive(Component)]
pub struct MenuSearch;
#[derive(Component)]
pub struct MenuList;

// --- palette look ----------------------------------------------------------
const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const SELECTED: Color = Color::srgb(0.24, 0.54, 0.93);
const ITEM: Color = Color::srgb(0.82, 0.82, 0.85);
const GRAYED: Color = Color::srgba(0.5, 0.5, 0.55, 0.55);
const HINT: Color = Color::srgba(0.6, 0.6, 0.6, 0.8);
const SEARCH_BG: Color = Color::srgb(0.16, 0.16, 0.18);
const SEARCH_TEXT: Color = Color::srgb(0.92, 0.92, 0.95);
const SEARCH_PLACEHOLDER: Color = Color::srgba(0.55, 0.55, 0.6, 0.85);
const CURSOR_GLYPH: &str = "_";

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
                    MenuSearch,
                    // Seed the placeholder text here so the first frame the
                    // menu opens shows the right thing — the search bar's
                    // text is updated through `Commands` (one frame deferred)
                    // to dodge Bevy's component-level conflict check.
                    Text::new("type to search\u{2026}"),
                    TextFont::from_font_size(FONT),
                    TextColor(SEARCH_PLACEHOLDER),
                    Node {
                        width: percent(100),
                        padding: UiRect::all(px(5)),
                        border_radius: BorderRadius::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(SEARCH_BG),
                ));
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
                    Text::new("type to search   arrows navigate   enter select   esc close"),
                    TextFont::from_font_size(FONT),
                    TextColor(HINT),
                ));
            });
        });
}

/// One list row: full-width, highlighted when `selected`, dimmed when `grayed`.
fn item(c: &mut ChildSpawnerCommands, label: &str, selected: bool, grayed: bool) {
    let (bg, fg) = if selected {
        (SELECTED, Color::WHITE)
    } else if grayed {
        (Color::NONE, GRAYED)
    } else {
        (Color::NONE, ITEM)
    };
    c.spawn((
        Text::new(label),
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

/// The render cache key: rebuilding the rows is only worth it when one of
/// `(command, step, cursor, query)` changes (the buildings panel's cache idea).
type CacheKey = (Option<usize>, usize, usize, String);

/// Render the stored list: toggle the overlay and rebuild the rows only when
/// `(command, step, index, query)` moves (the buildings panel's table cache idea).
/// Reads the list [`input`] stored on the resource, not the world.
pub fn update(
    menu: Res<CommandMenu>,
    mut root: Single<&mut Node, With<MenuRoot>>,
    mut title: Single<&mut Text, With<MenuTitle>>,
    // The search bar's text is updated through `Commands` rather than a second
    // `Single<&mut Text, …>`: two `Single`s both wanting to mutate `Text` trip
    // Bevy's component-level conflict check, even though their markers target
    // disjoint entities. Commands is deferred, so there's a one-frame lag —
    // invisible at the typing rate this menu handles.
    search: Single<Entity, With<MenuSearch>>,
    list: Single<Entity, With<MenuList>>,
    mut commands: Commands,
    mut cache: Local<Option<CacheKey>>,
) {
    root.display = if menu.open { Display::Flex } else { Display::None };
    if !menu.open {
        *cache = None;
        return;
    }
    let key = (menu.command, menu.step, menu.index, menu.query.clone());
    if *cache == Some(key.clone()) {
        return;
    }
    *cache = Some(key);

    title.0 = menu.title.clone();
    let is_empty = menu.query.is_empty();
    let text_str = if is_empty {
        "type to search\u{2026}".to_string()
    } else {
        format!("{query}{CURSOR_GLYPH}", query = menu.query)
    };
    let color = if is_empty {
        SEARCH_PLACEHOLDER
    } else {
        SEARCH_TEXT
    };
    commands.entity(*search).insert((Text::new(text_str), TextColor(color)));
    let items = &menu.items;
    let matches = &menu.matches;
    let index = menu.index;
    commands.entity(*list).despawn_children().with_children(|c| {
        // ponytail: no step-back navigation. An empty step is a dead end until
        // the player escapes; add Back/Bksp if commands grow multi-step paths.
        let any_match = matches.iter().any(|&m| m);
        if items.is_empty() {
            item(c, "(nothing to choose)", false, false);
        } else {
            for (i, mi) in items.iter().enumerate() {
                // `refresh` reorders `items` so matches sit at the top; the
                // `matches` vec tells `update` which rows are the dimmed
                // non-matches at the bottom. When nothing matches the cursor
                // has nowhere to land, so no row gets the blue highlight.
                let grayed = !matches[i];
                let selected = any_match && i == index;
                item(c, &mi.label, selected, grayed);
            }
        }
    });
}

/// Exclusive: open on **C**, navigate, dispatch on the final **Enter**.
/// Exclusive because it computes the on-screen list via `&World` (each command
/// decides its own steps/queries) and the last step calls the command's
/// `execute`, an `&mut World` method.
pub fn input(world: &mut World) {
    let (toggle, up, down, enter, escape) = {
        let keys = world.resource::<ButtonInput<KeyCode>>();
        (
            keys.just_pressed(KeyCode::KeyC),
            keys.just_pressed(KeyCode::ArrowUp),
            keys.just_pressed(KeyCode::ArrowDown),
            keys.just_pressed(KeyCode::Enter),
            keys.just_pressed(KeyCode::Escape),
        )
    };

    // Drain typed characters and backspace out of the `KeyboardInput` message
    // stream while the menu is open. The cursor lives on the resource so
    // re-runs of this exclusive system don't re-process the same events
    // twice. We clone it out so we can hold the messages borrow alongside
    // without conflicting with the `CommandMenu` mutable borrow. Backspace is
    // read here (not via `ButtonInput::just_pressed`) so the OS auto-repeat
    // stream flows through — hold-to-delete works.
    let mut typed = String::new();
    let mut backspace = false;
    {
        let mut cursor = world.resource_mut::<CommandMenu>().kb_cursor.clone();
        {
            let messages = world.resource::<Messages<KeyboardInput>>();
            for event in cursor.read(messages) {
                if !event.state.is_pressed() {
                    continue;
                }
                if event.key_code == KeyCode::Backspace {
                    backspace = true;
                // Skip control keys that `ButtonInput<KeyCode>` already routes
                // (Enter → pick, arrows → navigate, Esc → close): their
                // `KeyboardInput` events sometimes carry text (Enter → "\r"),
                // and appending it to the query would put a newline in the bar.
                } else if event.key_code != KeyCode::Enter
                    && event.key_code != KeyCode::Tab
                    && let Some(text) = &event.text
                {
                    typed.push_str(text);
                }
            }
        }
        world.resource_mut::<CommandMenu>().kb_cursor = cursor;
    }

    if !world.resource::<CommandMenu>().open {
        if toggle {
            open_menu(world, None, 0, Vec::new());
        }
        return;
    }

    if escape {
        close(world);
        return;
    }

    if backspace || !typed.is_empty() {
        {
            let mut m = world.resource_mut::<CommandMenu>();
            if backspace {
                m.query.pop();
            }
            m.query.push_str(&typed);
        }
        refresh(world);
        return;
    }

    if up || down {
        navigate(world, up);
        refresh(world);
        return;
    }

    if enter {
        // Gate Enter on the cursor pointing at a match — when the search has
        // no hits, the dimmed rows aren't selectable.
        let cursor_at_match = {
            let m = world.resource::<CommandMenu>();
            m.matches.get(m.index).copied().unwrap_or(false)
        };
        if cursor_at_match {
            pick(world);
            // `pick` closes the menu on the final step; only refresh if it stayed open.
            if world.resource::<CommandMenu>().open {
                refresh(world);
            }
        }
    }
}

/// Move the cursor up (`up = true`) or down through the on-screen list.
/// When the query is non-empty, navigation skips non-matches so the player
/// never lands on a dimmed row. With an empty query every item matches and
/// the cursor walks the full list.
fn navigate(world: &mut World, up: bool) {
    let (matches, len) = {
        let m = world.resource::<CommandMenu>();
        (m.matches.clone(), m.items.len())
    };
    if len == 0 {
        return;
    }
    let cur = world.resource::<CommandMenu>().index;
    let step: i64 = if up { -1 } else { 1 };
    let mut idx = cur as i64;
    for _ in 0..len {
        idx = (idx + step).rem_euclid(len as i64);
        if matches[idx as usize] {
            break;
        }
    }
    world.resource_mut::<CommandMenu>().index = idx as usize;
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
            m.query.clear();
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
        // Each panel gets a fresh search — the filter from the previous step
        // (e.g. typed at the top-level command list) doesn't carry forward.
        m.query.clear();
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

/// Recompute the on-screen list + title for the current `(command, step,
/// query)` and clamp the cursor to a matching row.
fn refresh(world: &mut World) {
    let items = current_items(world);
    let title = current_title(world);
    let needle = world.resource::<CommandMenu>().query.to_lowercase();
    // Reorder so matches sit at the top of the list and the dimmed rows
    // follow. `current_items` always returns them in their natural order;
    // the palette owns the visual reorder so each command stays oblivious.
    let mut reordered: Vec<(MenuItem, bool)> = Vec::with_capacity(items.len());
    let mut matches_buf: Vec<(MenuItem, bool)> = Vec::new();
    let mut nonmatches_buf: Vec<(MenuItem, bool)> = Vec::new();
    for item in items {
        let hit = needle.is_empty() || item.label.to_lowercase().contains(&needle);
        if hit {
            matches_buf.push((item, true));
        } else {
            nonmatches_buf.push((item, false));
        }
    }
    reordered.extend(matches_buf);
    reordered.extend(nonmatches_buf);
    let (items, matches): (Vec<MenuItem>, Vec<bool>) = reordered.into_iter().unzip();

    let mut m = world.resource_mut::<CommandMenu>();
    m.items = items;
    m.matches = matches;
    m.title = title;
    // Clamp into range, then snap to the first match if the cursor landed on a
    // non-match (e.g. the player typed something that filters out index 0).
    if m.index >= m.items.len() {
        m.index = m.items.len().saturating_sub(1);
    }
    if !m.matches.is_empty()
        && !m.matches[m.index]
        && let Some(first_match) = m.matches.iter().position(|&x| x)
    {
        m.index = first_match;
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
    m.query.clear();
    m.choices.clear();
}

/// Open the palette into `command` at `step` with `choices` already made. The
/// **C** top-level open uses `command = None`, step 0.
fn open_menu(world: &mut World, command: Option<usize>, step: usize, choices: Vec<Choice>) {
    {
        let mut m = world.resource_mut::<CommandMenu>();
        m.open = true;
        m.command = command;
        m.step = step;
        m.index = 0;
        m.query.clear();
        m.choices = choices;
    }
    refresh(world);
}

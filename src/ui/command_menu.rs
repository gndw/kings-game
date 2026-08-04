//! The command palette: a spotlight-style modal that launches player commands.
//!
//! Press **C** to open. The command list is navigated with up/down; **Enter**
//! drills in (command → a land you rule → a building kind → builds). **Escape**
//! closes. While open it captures the arrows (so the map selection doesn't
//! move) and Escape (so the game doesn't quit) — both gated by reading
//! [`CommandMenu::open`] from `app::input` and `ui::map::update_input`.
//!
//! The palette builds a [`Command`] and routes it through [`apply`], the same
//! exclusive path the old key-B handler used, so validation and chronicle
//! logging are unchanged.
//!
//! [`apply`]: crate::commands::apply

use super::{FONT, TITLE};
use crate::app::Game;
use crate::commands::{Command, apply};
use crate::ecs::{HeldBy, LandName, Leads, Registry, StringId};
use crate::resources::buildings::BuildingDefs;
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;

/// The palette's state. Only [`CommandMenu::open`] is read outside this module
/// (by `app::input` and `ui::map::update_input`, to yield `esc`/arrows).
#[derive(Resource)]
pub struct CommandMenu {
    pub open: bool,
    stage: Stage,
    index: usize,
    /// The land chosen at the [`Stage::Lands`] step, carried into
    /// [`Stage::Buildings`].
    land_id: Option<String>,
}

impl Default for CommandMenu {
    fn default() -> Self {
        CommandMenu { open: false, stage: Stage::Commands, index: 0, land_id: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The top-level command list.
    Commands,
    /// Pick a land to build on.
    Lands,
    /// Pick a building kind.
    Buildings,
}

impl Stage {
    fn title(self) -> &'static str {
        match self {
            Stage::Commands => "Command",
            Stage::Lands => "Select a land",
            Stage::Buildings => "Select a building",
        }
    }
}

/// The commands on the top-level list, in display order. Index 0 is
/// `ConstructBuilding`. Adding one = a line here + its [`Stage::Commands`]
/// `Enter` arm.
const COMMANDS: &[&str] = &["Construct Building"];

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
        (SELECTED, Color::WHITE, "›  ")
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

/// The lands the player rules (can build on): player → `Leads` → kingdom →
/// lands whose `HeldBy` is that kingdom, in spawn (content) order. The same
/// walk `ui::map` and `ui::legend` do for "own holdings".
fn own_holdings(
    registry: &Registry,
    game: &Game,
    leads: &Query<&Leads>,
    lands: &Query<(&StringId, &LandName, &HeldBy)>,
) -> Vec<(String, String)> {
    let kingdom = registry
        .get(&game.ctx.player_character_id)
        .and_then(|pe| leads.get(pe).ok())
        .map(|l| l.kingdom());
    lands
        .iter()
        .filter(|(_, _, held)| Some(held.0) == kingdom)
        .map(|(sid, name, _)| (sid.0.clone(), name.0.clone()))
        .collect()
}

/// Toggle the overlay and rebuild the list only when the stage/cursor moves;
/// identical (stage, index) frames leave the rows alone (the legend's table
/// cache idea).
#[allow(clippy::type_complexity)]
pub fn update(
    menu: Res<CommandMenu>,
    game: Res<Game>,
    registry: Res<Registry>,
    defs: Res<BuildingDefs>,
    leads: Query<&Leads>,
    lands: Query<(&StringId, &LandName, &HeldBy)>,
    mut root: Single<&mut Node, With<MenuRoot>>,
    mut title: Single<&mut Text, With<MenuTitle>>,
    list: Single<Entity, With<MenuList>>,
    mut commands: Commands,
    mut cache: Local<Option<(Stage, usize)>>,
) {
    root.display = if menu.open { Display::Flex } else { Display::None };
    if !menu.open {
        *cache = None;
        return;
    }
    let key = (menu.stage, menu.index);
    if *cache == Some(key) {
        return;
    }
    *cache = Some(key);

    title.0 = menu.stage.title().to_string();
    commands
        .entity(*list)
        .despawn_children()
        .with_children(|c| match menu.stage {
            Stage::Commands => {
                for (i, name) in COMMANDS.iter().enumerate() {
                    item(c, *name, i == menu.index);
                }
            }
            Stage::Lands => {
                let rows = own_holdings(&registry, &game, &leads, &lands);
                if rows.is_empty() {
                    item(c, "(no lands you rule)", false);
                } else {
                    for (i, (_, name)) in rows.iter().enumerate() {
                        item(c, name, i == menu.index);
                    }
                }
            }
            Stage::Buildings => {
                if defs.0.is_empty() {
                    item(c, "(no buildings defined)", false);
                } else {
                    for (i, def) in defs.0.values().enumerate() {
                        item(c, &format!("{}  ({}g)", def.name, def.construction_price), i == menu.index);
                    }
                }
            }
        });
}

/// Exclusive: open on **C**, navigate the list, dispatch on the final
/// **Enter**. Exclusive because the last step calls [`apply`], an `&mut World`
/// free function.
///
/// [`apply`]: crate::commands::apply
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
    let open = world.resource::<CommandMenu>().open;
    if !open {
        if toggle {
            let mut m = world.resource_mut::<CommandMenu>();
            m.open = true;
            m.stage = Stage::Commands;
            m.index = 0;
            m.land_id = None;
        }
        return;
    }
    if escape {
        close(world);
        return;
    }
    let stage = world.resource::<CommandMenu>().stage;
    if up || down {
        let len = match stage {
            Stage::Commands => COMMANDS.len(),
            Stage::Lands => own_holdings_world(world).len(),
            Stage::Buildings => world.resource::<BuildingDefs>().0.len(),
        };
        if len > 0 {
            let idx = world.resource::<CommandMenu>().index;
            let next = if up { (idx + len - 1) % len } else { (idx + 1) % len };
            world.resource_mut::<CommandMenu>().index = next;
        }
        return;
    }
    if enter {
        match stage {
            Stage::Commands => {
                let idx = world.resource::<CommandMenu>().index;
                if idx < COMMANDS.len() {
                    let mut m = world.resource_mut::<CommandMenu>();
                    m.stage = Stage::Lands;
                    m.index = 0;
                }
            }
            Stage::Lands => {
                let idx = world.resource::<CommandMenu>().index;
                if let Some((id, _)) = own_holdings_world(world).into_iter().nth(idx) {
                    let mut m = world.resource_mut::<CommandMenu>();
                    m.land_id = Some(id);
                    m.stage = Stage::Buildings;
                    m.index = 0;
                }
            }
            Stage::Buildings => {
                let idx = world.resource::<CommandMenu>().index;
                let land_id = world.resource::<CommandMenu>().land_id.clone();
                let actor = world.resource::<Game>().ctx.player_character_id.clone();
                let def_id = world
                    .resource::<BuildingDefs>()
                    .0
                    .get_index(idx)
                    .map(|(k, _)| k.clone());
                let (Some(land_id), Some(def_id)) = (land_id, def_id) else {
                    return;
                };
                close(world);
                apply(world, &actor, Command::ConstructBuilding { land_id, def_id });
            }
        }
    }
}

fn close(world: &mut World) {
    let mut m = world.resource_mut::<CommandMenu>();
    m.open = false;
    m.stage = Stage::Commands;
    m.index = 0;
    m.land_id = None;
}

/// `&mut World` mirror of [`own_holdings`] for the exclusive input path (which
/// can't take `Query` system params).
fn own_holdings_world(world: &mut World) -> Vec<(String, String)> {
    let kingdom = world
        .resource::<Registry>()
        .get(&world.resource::<Game>().ctx.player_character_id)
        .and_then(|pe| world.get::<Leads>(pe).map(|l| l.kingdom()));
    let mut q = world.query::<(&StringId, &LandName, &HeldBy)>();
    q.iter(world)
        .filter(|(_, _, held)| Some(held.0) == kingdom)
        .map(|(sid, name, _)| (sid.0.clone(), name.0.clone()))
        .collect()
}

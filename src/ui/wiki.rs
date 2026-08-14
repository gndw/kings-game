//! The wiki window: a modal the player opens with **W** to read the world's
//! information. Today the only section is **Houses**: a list of every house
//! in the loaded mods, with a family tree shown on selection.
//!
//! Style mirrors the command palette: full-screen backdrop, centered window,
//! `GlobalZIndex` above the panels, `Esc` to close. A new `InputLayer::Wiki`
//! gates root-layer keys while it's open.

use super::FONT;
use crate::ecs::{
    CharacterDateOfBirth, CharacterHasFather, CharacterHasFatheredChildren, CharacterHasHusband,
    CharacterHasMother, CharacterHasMotheredChildren, CharacterHasWife, CharacterName,
    CharacterOfHouse, HouseName, Registry,
};
use crate::game::aging::age;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::resources::input_layer::InputLayer;
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

// --- shell components ---------------------------------------------------

#[derive(Component)]
pub struct WikiUiRoot;

#[derive(Component)]
pub struct WikiBody;

#[derive(Component)]
pub struct WikiBackHint;

// --- context ------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum WikiView {
    #[default]
    HouseList,
    HouseTree(String),
}

#[derive(Clone)]
pub(crate) struct HouseEntry {
    id: String,
    name: String,
}

#[derive(Resource, Default)]
pub struct WikiUiContext {
    pub view: WikiView,
    pub(crate) houses: Vec<HouseEntry>,
    pub list_index: usize,
    /// Last view the body text was rendered for. `update` skips rebuilding
    /// when this matches — the tree is static, no need to reformat every frame.
    pub(crate) last_rendered: Option<WikiView>,
}

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const WINDOW: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const TITLE_COLOR: Color = Color::srgb(0.96, 0.96, 0.96);
const HINT_COLOR: Color = Color::srgba(0.75, 0.75, 0.80, 0.85);
const BACK_BG: Color = Color::srgb(0.18, 0.14, 0.10);
const BACK_TEXT: Color = Color::srgb(0.95, 0.75, 0.55);
const BODY_COLOR: Color = Color::srgb(0.96, 0.96, 0.98);
const Z_INDEX: i32 = 100;

/// Per-character bundle we collect once per tree render. Tuple alias for the
/// Query type so the type signature doesn't sprawl.
type CharBundle = (
    Entity,
    &'static CharacterName,
    &'static CharacterDateOfBirth,
    &'static CharacterOfHouse,
    Option<&'static CharacterHasFather>,
    Option<&'static CharacterHasMother>,
    Option<&'static CharacterHasHusband>,
    Option<&'static CharacterHasWife>,
    Option<&'static CharacterHasFatheredChildren>,
    Option<&'static CharacterHasMotheredChildren>,
);

// --- startup ------------------------------------------------------------

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
                    Text::new("W: open / close   \u{2191}\u{2193}: select   Enter: open house   Esc: back"),
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
                        WikiBackHint,
                        Text::new("\u{2190} Back to Houses (Esc)"),
                        TextFont::from_font_size(FONT - 2.0),
                        TextColor(BACK_TEXT),
                        Node {
                            display: Display::None,
                            padding: UiRect::all(px(4)),
                            ..default()
                        },
                        BackgroundColor(BACK_BG),
                    ));
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

// --- open / close -------------------------------------------------------

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
    let houses = collect_houses(world);
    let mut ctx = world.resource_mut::<WikiUiContext>();
    ctx.houses = houses;
    ctx.list_index = 0;
    ctx.view = WikiView::HouseList;
    ctx.last_rendered = None;
    *world.resource_mut::<InputLayer>() = InputLayer::Wiki;
}

pub fn close_wiki(world: &mut World) {
    hide_panel(world);
    let mut ctx = world.resource_mut::<WikiUiContext>();
    ctx.last_rendered = None;
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

fn set_back_hint_visible(world: &mut World, visible: bool) {
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in world
        .query_filtered::<&mut Node, With<WikiBackHint>>()
        .iter_mut(world)
    {
        node.display = display;
    }
}

fn collect_houses(world: &World) -> Vec<HouseEntry> {
    let registry = world.resource::<Registry>();
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();
    for (id, entity) in &registry.by_id {
        let Some(name) = world.get::<HouseName>(*entity) else {
            continue;
        };
        by_name.insert(name.0.clone(), id.clone());
    }
    by_name
        .into_iter()
        .map(|(name, id)| HouseEntry { id, name })
        .collect()
}

// --- input --------------------------------------------------------------

pub fn wiki_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Wiki
}

/// Per-frame input. Exclusive because closing the wiki flips the input layer
/// through `&mut World`. Mirrors `ui::command_menu::input`.
pub fn input(world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();

    let mut delta: i32 = 0;
    let mut enter = false;
    let mut esc = false;
    if keys.just_pressed(KeyCode::ArrowDown) {
        delta += 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        delta -= 1;
    }
    if keys.just_pressed(KeyCode::Enter) {
        enter = true;
    }
    if keys.just_pressed(KeyCode::Escape) {
        esc = true;
    }

    enum Action {
        Close,
        Back,
        Drill,
        Move,
        None,
    }
    let mut action = Action::None;

    {
        let mut ctx = world.resource_mut::<WikiUiContext>();
        match ctx.view {
            WikiView::HouseList if esc => action = Action::Close,
            WikiView::HouseTree(_) if esc => action = Action::Back,
            _ => {}
        }
        if delta != 0 && !ctx.houses.is_empty() {
            let n = ctx.houses.len() as i32;
            ctx.list_index = ((ctx.list_index as i32 + delta).rem_euclid(n)) as usize;
            ctx.last_rendered = None;
            action = Action::Move;
        }
        if enter && matches!(ctx.view, WikiView::HouseList) {
            if let Some(house) = ctx.houses.get(ctx.list_index).cloned() {
                ctx.view = WikiView::HouseTree(house.id);
                ctx.last_rendered = None;
                action = Action::Drill;
            }
        }
    }

    match action {
        Action::Close => close_wiki(world),
        Action::Back => {
            {
                let mut ctx = world.resource_mut::<WikiUiContext>();
                ctx.view = WikiView::HouseList;
                ctx.last_rendered = None;
            }
            set_back_hint_visible(world, false);
        }
        Action::Drill => {
            set_back_hint_visible(world, true);
        }
        Action::Move | Action::None => {}
    }
}

// --- update -------------------------------------------------------------

/// Rebuilds the body text only when the view changes, and updates the back
/// hint's visibility every frame (cheap).
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut ctx: ResMut<WikiUiContext>,
    registry: Res<Registry>,
    house_names: Query<&HouseName>,
    chars: Query<CharBundle>,
    calendar: Res<Calendar>,
    date: Res<Date>,
    mut body_text: Single<&mut Text, With<WikiBody>>,
    mut body_font: Single<&mut TextFont, With<WikiBody>>,
    mut nodes: Query<&mut Node, With<WikiBackHint>>,
) {
    let view = ctx.view.clone();
    let last = ctx.last_rendered.clone();
    if last.as_ref() != Some(&view) {
        match &view {
            WikiView::HouseList => {
                let mut text = String::new();
                text.push_str("Houses\n");
                text.push_str("------\n");
                for (i, h) in ctx.houses.iter().enumerate() {
                    let marker = if i == ctx.list_index { "> " } else { "  " };
                    let _ = writeln!(text, "{marker}{}", h.name);
                }
                if ctx.houses.is_empty() {
                    text.push_str("(no houses loaded)\n");
                }
                body_text.0 = text;
                body_font.font_size = FONT.into();
            }
            WikiView::HouseTree(house_id) => {
                let text =
                    render_tree(house_id, &registry, &house_names, &chars, &calendar, &date);
                body_text.0 = text;
                body_font.font_size = (FONT - 1.0).into();
            }
        }
        ctx.last_rendered = Some(view.clone());
    }

    let visible = matches!(view, WikiView::HouseTree(_));
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes {
        node.display = display;
    }
}

// --- tree renderer ------------------------------------------------------

#[derive(Clone)]
struct Person {
    #[allow(dead_code)]
    entity: Entity,
    name: String,
    age: u32,
    father: Option<Entity>,
    mother: Option<Entity>,
    spouse: Option<Entity>,
    children: Vec<Entity>,
}

#[allow(clippy::too_many_arguments)]
fn render_tree(
    house_id: &str,
    registry: &Registry,
    house_names: &Query<&HouseName>,
    chars: &Query<CharBundle>,
    calendar: &Calendar,
    date: &Date,
) -> String {
    let Some(house_entity) = registry.get(house_id) else {
        return format!("(unknown house `{house_id}`)\n");
    };

    let mut people: HashMap<Entity, Person> = HashMap::new();
    for (entity, name, dob, house, father, mother, husband, wife, fathered, mothered) in
        chars.iter()
    {
        if house.0 != house_entity {
            continue;
        }
        let age = age(&dob.0, date, calendar);
        let spouse = husband.map(|h| h.0).or_else(|| wife.map(|w| w.wife()));
        let mut children: Vec<Entity> = Vec::new();
        let mut added: HashSet<Entity> = HashSet::new();
        if let Some(f) = fathered {
            for c in f.children() {
                if added.insert(*c) {
                    children.push(*c);
                }
            }
        }
        if let Some(m) = mothered {
            for c in m.children() {
                if added.insert(*c) {
                    children.push(*c);
                }
            }
        }
        people.insert(
            entity,
            Person {
                entity,
                name: name.0.clone(),
                age,
                father: father.map(|f| f.0),
                mother: mother.map(|m| m.0),
                spouse,
                children,
            },
        );
    }

    if people.is_empty() {
        return format!("(no members in {house_id})\n");
    }

    let house_label = house_names
        .get(house_entity)
        .ok()
        .map(|n| n.0.clone())
        .unwrap_or_else(|| house_id.to_string());

    let spouse_house_label = |spouse_e: Entity| -> Option<String> {
        let (_, _, _, sp_house, _, _, _, _, _, _) = chars.get(spouse_e).ok()?;
        house_names.get(sp_house.0).ok().map(|n| n.0.clone())
    };

    let mut roots: Vec<Entity> = people
        .iter()
        .filter_map(|(e, p)| {
            let father_in = p.father.map(|f| people.contains_key(&f)).unwrap_or(false);
            let mother_in = p.mother.map(|m| people.contains_key(&m)).unwrap_or(false);
            (!father_in && !mother_in).then_some(*e)
        })
        .collect();
    roots.sort_by_key(|e| e.to_bits());

    let mut out = String::new();
    let _ = writeln!(out, "House {house_label}\n");

    let mut seen: HashSet<Entity> = HashSet::new();
    let mut drawn_couples: HashSet<(u64, u64)> = HashSet::new();
    let couple_key = |a: Entity, b: Entity| -> (u64, u64) {
        let (lo, hi) = if a.to_bits() < b.to_bits() {
            (a.to_bits(), b.to_bits())
        } else {
            (b.to_bits(), a.to_bits())
        };
        (lo, hi)
    };

    let n_roots = roots.len();
    for (i, root) in roots.iter().enumerate() {
        if seen.contains(root) {
            continue;
        }
        let partner = people
            .get(root)
            .and_then(|p| p.spouse)
            .filter(|s| people.contains_key(s));
        if let Some(partner) = partner {
            let key = couple_key(*root, partner);
            if drawn_couples.contains(&key) {
                continue;
            }
            drawn_couples.insert(key);
            seen.insert(*root);
            seen.insert(partner);
            draw_couple_line(&mut out, "", *root, Some(partner), &people, &spouse_house_label);
            let children = merged_children(*root, &people);
            let m = children.len();
            for (j, child) in children.iter().enumerate() {
                draw_subtree(
                    &mut out,
                    *child,
                    "",
                    j == m - 1,
                    &people,
                    &spouse_house_label,
                    &couple_key,
                    &mut seen,
                    &mut drawn_couples,
                );
            }
        } else {
            draw_subtree(
                &mut out,
                *root,
                "",
                i == n_roots - 1,
                &people,
                &spouse_house_label,
                &couple_key,
                &mut seen,
                &mut drawn_couples,
            );
        }
    }

    out
}

fn merged_children(person: Entity, people: &HashMap<Entity, Person>) -> Vec<Entity> {
    people.get(&person).map(|p| p.children.clone()).unwrap_or_default()
}

fn draw_couple_line(
    out: &mut String,
    prefix: &str,
    primary: Entity,
    spouse: Option<Entity>,
    people: &HashMap<Entity, Person>,
    spouse_house_label: &dyn Fn(Entity) -> Option<String>,
) {
    let Some(p) = people.get(&primary) else {
        return;
    };
    let connector = if prefix.is_empty() { "" } else { "├─ " };
    let primary_str = format!("{} (age {})", p.name, p.age);

    let spouse_str = match spouse.and_then(|s| people.get(&s)) {
        Some(sp) => format!(" \u{2500} {} (age {})", sp.name, sp.age),
        None => p
            .spouse
            .filter(|s| !people.contains_key(s))
            .and_then(|s| spouse_house_label(s).map(|h| format!(" \u{2500} of {h}")))
            .unwrap_or_default(),
    };

    let _ = writeln!(out, "{prefix}{connector}{primary_str}{spouse_str}");
}

#[allow(clippy::too_many_arguments)]
fn draw_subtree(
    out: &mut String,
    person: Entity,
    prefix: &str,
    is_last_sibling: bool,
    people: &HashMap<Entity, Person>,
    spouse_house_label: &dyn Fn(Entity) -> Option<String>,
    couple_key: &dyn Fn(Entity, Entity) -> (u64, u64),
    seen: &mut HashSet<Entity>,
    drawn_couples: &mut HashSet<(u64, u64)>,
) {
    if !seen.insert(person) {
        return;
    }
    let Some(p) = people.get(&person) else {
        return;
    };

    let connector = if is_last_sibling { "└─ " } else { "├─ " };
    let line_prefix = format!("{prefix}{connector}");
    let partner = p.spouse.filter(|s| people.contains_key(s));
    let already_drawn = partner
        .map(|s| drawn_couples.contains(&couple_key(person, s)))
        .unwrap_or(false);
    match (partner, already_drawn) {
        (Some(s), false) => {
            drawn_couples.insert(couple_key(person, s));
            seen.insert(s);
            draw_couple_line_with_prefix(
                out,
                &line_prefix,
                person,
                Some(s),
                people,
                spouse_house_label,
            );
        }
        _ => {
            draw_couple_line_with_prefix(
                out,
                &line_prefix,
                person,
                None,
                people,
                spouse_house_label,
            );
        }
    }

    let child_indent = if is_last_sibling { "    " } else { "│   " };
    let child_prefix = format!("{prefix}{child_indent}");
    let children = merged_children(person, people);
    let m = children.len();
    for (j, child) in children.iter().enumerate() {
        draw_subtree(
            out,
            *child,
            &child_prefix,
            j == m - 1,
            people,
            spouse_house_label,
            couple_key,
            seen,
            drawn_couples,
        );
    }
}

fn draw_couple_line_with_prefix(
    out: &mut String,
    prefix: &str,
    primary: Entity,
    spouse: Option<Entity>,
    people: &HashMap<Entity, Person>,
    spouse_house_label: &dyn Fn(Entity) -> Option<String>,
) {
    let Some(p) = people.get(&primary) else {
        return;
    };
    let primary_str = format!("{} (age {})", p.name, p.age);
    let spouse_str = match spouse.and_then(|s| people.get(&s)) {
        Some(sp) => format!(" \u{2500} {} (age {})", sp.name, sp.age),
        None => p
            .spouse
            .filter(|s| !people.contains_key(s))
            .and_then(|s| spouse_house_label(s).map(|h| format!(" \u{2500} of {h}")))
            .unwrap_or_default(),
    };
    let _ = writeln!(out, "{prefix}{primary_str}{spouse_str}");
}

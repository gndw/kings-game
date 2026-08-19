//! The kingdom panel: a right-docked panel the player opens with **Enter** to
//! pin a kingdom. Stays pinned as the map selection moves; Enter on a
//! different kingdom switches the pinned kingdom, and Enter on the pinned
//! kingdom closes the panel.
//!
//! Rendered sections: kingdom name, land, ruler, courtiers, wars, armies,
//! buildings. Building row colors match the spec: red when the levy is raised,
//! yellow when the levy is below max, gold for profit, green for max levy,
//! gray for upkeep.

use super::{FONT, TITLE, spawn_span};
use crate::app::Game;
use crate::ecs::character::{
    CharacterDateOfBirth, CharacterGender, CharacterHasFather, CharacterHasHusband,
    CharacterHasMother, CharacterName, CharacterOfHouse, MemoryKind, MemoryOfCharacter,
    MemoryTowardCharacter, MemoryUntilDate,
};
use crate::ecs::house::HouseName;
use crate::ecs::courtier::CourtierOfCharacter;
use crate::ecs::{
    KingdomHasArmies, KingdomHasCourtiers, KingdomHasWarsAttacking, KingdomHasWarsDefending,
    KingdomHold, KingdomLedBy, KingdomName, LandHasBuildings, LandName, LandHeldBy, Registry,
    StringId,
};
use crate::ecs::army::{
    ArmyHasMarching, ArmyHasSiege, ArmyLevy, ArmyMarching, ArmyMaxLevy, ArmyName, ArmyOnLand,
    ArmyStatus,
};
use crate::ecs::building::{
    BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus,
};
use crate::ecs::marching::{
    MarchingArrivedDate, MarchingOnRoad, MarchingStatus, MarchingToLand,
};
use crate::ecs::road::RoadDistanceDays;
use crate::ecs::siege::SiegeProgress;
use crate::ecs::war::{WarBeginDate, WarName};
use crate::helper::age_helper::age;
use crate::helper::opinion_helper::opinion_color;
use crate::resources::buildings::BuildingDefs;
use crate::resources::calendar::Calendar;
use crate::resources::date::Date;
use crate::resources::input_layer::InputLayer;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// The full panel shell (root node). Hidden by default; flipped open by
/// `apply_toggle` once the player presses Enter on a kingdom.
#[derive(Component)]
pub struct KingdomUIRoot;

/// The dynamic body of the panel — every section under the title lives here.
#[derive(Component)]
pub struct KingdomUIBody;

/// Tracks which kingdom the panel is currently pinned to, if any. Lives in
/// the world so the input handler and the renderer can each read it without
/// walking the UI node tree.
#[derive(Resource, Default)]
pub struct KingdomUiContext {
    pub pinned_kingdom_id: Option<String>,
}

const PANEL_BG: Color = Color::srgb(0.10, 0.10, 0.12);
const BORDER: Color = Color::srgba(0.6, 0.6, 0.65, 0.5);
const Z_INDEX: i32 = 50;

const RAISED_RED: Color = Color::Srgba(css::RED);
const PARTIAL_YELLOW: Color = Color::Srgba(css::YELLOW);
const LEVY_GREEN: Color = Color::Srgba(css::GREEN);
const UPKEEP_GRAY: Color = Color::srgb(0.55, 0.55, 0.55);
const BUILDING_GRAY: Color = Color::srgba(0.55, 0.55, 0.55, 1.0);
const GOLD_COLOR: Color = Color::Srgba(css::GOLD);

/// Spawn the panel shell once, hidden. Right-docked so the map keeps its
/// area; width is a fixed percent rather than a flex sibling so the panel
/// doesn't compete with the camera for layout space.
pub fn startup(mut commands: Commands) {
    commands
        .spawn((
            KingdomUIRoot,
            Node {
                position_type: PositionType::Absolute,
                right: px(0),
                top: px(0),
                bottom: px(0),
                width: percent(35),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(4),
                border: UiRect::all(px(1)),
                overflow: Overflow::clip(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(BORDER),
            GlobalZIndex(Z_INDEX),
        ))
        .with_children(|win| {
            win.spawn((
                Text::new("KINGDOM"),
                TextFont::from_font_size(FONT + 2.0),
                TextColor(TITLE),
            ));
            win.spawn((
                KingdomUIBody,
                Text::new(""),
                TextFont::from_font_size(FONT),
                TextColor(Color::WHITE),
            ));
        });
}

/// Run condition: the root input layer is active (no modal in the way).
/// Exposed so `main.rs` can attach it via `run_if`, but the same check also
/// sits inside [`input`] — defense in depth, so a future system re-ordering
/// can't silently fire Enter on the kingdom while the palette is open.
pub fn root_layer_active(layer: Res<InputLayer>) -> bool {
    *layer == InputLayer::Root
}

/// Enter at the root layer toggles the panel: same kingdom → close,
/// different kingdom → switch, no panel → open on the selected land's
/// kingdom. Gated to `InputLayer::Root` inline so the run-if in `main.rs`
/// isn't the sole gate — defensive against any future system reorder.
/// Exclusive because the toggle pokes the resource and the UI tree in
/// one shot; Bevy exclusive access only routes through `&mut World`.
pub fn input(world: &mut World) {
    if *world.resource::<InputLayer>() != InputLayer::Root {
        return;
    }
    let enter = world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Enter);
    if !enter {
        return;
    }
    let Some(target_e) = selected_kingdom(world) else {
        return;
    };
    let target_id = world
        .get::<StringId>(target_e)
        .map(|s| s.0.clone())
        .unwrap_or_default();
    let action = match world.resource::<KingdomUiContext>().pinned_kingdom_id.clone() {
        Some(id) if id == target_id => Toggle::Close,
        Some(_) => Toggle::Switch(target_id),
        None => Toggle::Open(target_id),
    };
    apply_toggle(world, action);
}

/// What `apply_toggle` will do — encoded so the action choice and the
/// application stay in one place each.
enum Toggle {
    Close,
    Open(String),
    Switch(String),
}

fn apply_toggle(world: &mut World, action: Toggle) {
    match action {
        Toggle::Close => {
            world.resource_mut::<KingdomUiContext>().pinned_kingdom_id = None;
            set_visible(world, false);
        }
        Toggle::Open(id) | Toggle::Switch(id) => {
            world.resource_mut::<KingdomUiContext>().pinned_kingdom_id = Some(id);
            set_visible(world, true);
        }
    }
}

fn set_visible(world: &mut World, visible: bool) {
    let Some(root) = world
        .query_filtered::<Entity, With<KingdomUIRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(root) {
        node.display = if visible { Display::Flex } else { Display::None };
    }
}

/// The kingdom holding the currently selected land. `None` when nothing's
/// selected or the land has no holder — `input` no-ops on `None` so the
/// player can't open an empty panel.
fn selected_kingdom(world: &World) -> Option<Entity> {
    let game = world.resource::<Game>();
    let registry = world.resource::<Registry>();
    let land_e = game
        .ctx
        .selected_land_id
        .as_deref()
        .and_then(|id| registry.get(id))?;
    world.get::<LandHeldBy>(land_e).map(|lh| lh.kingdom())
}

/// Rebuild the body text every frame from the pinned kingdom's live data.
/// Exclusive because 30+ system parameters would exceed Bevy's 16-tuple
/// param ceiling; helpers fetch data via `&World` instead.
pub fn update(world: &mut World) {
    let Some(body_e) = world
        .query_filtered::<Entity, With<KingdomUIBody>>()
        .iter(world)
        .next()
    else {
        return;
    };

    let pinned_id = world.resource::<KingdomUiContext>().pinned_kingdom_id.clone();
    let Some(pinned_id) = pinned_id else {
        world.entity_mut(body_e).despawn_children();
        return;
    };
    let Some(kingdom_e) = world.resource::<Registry>().get(&pinned_id) else {
        world.entity_mut(body_e).despawn_children();
        return;
    };

    // Pull the kingdom's metadata up front; everything below takes &World.
    let (kingdom_name, kingdom_led_by, kingdom_hold) = {
        let ent = world.entity(kingdom_e);
        let n = ent.get::<KingdomName>().map(|c| c.0.clone());
        let l = ent.get::<KingdomLedBy>().map(|c| c.0);
        let h = ent.get::<KingdomHold>().map(|c| c.0);
        (n, l, h)
    };
    let Some(kingdom_name) = kingdom_name else {
        world.entity_mut(body_e).despawn_children();
        return;
    };

    let player_e = {
        let game = world.resource::<Game>();
        let registry = world.resource::<Registry>();
        game.ctx
            .player_character_id
            .as_deref()
            .and_then(|id| registry.get(id))
    };

    let spans = render_kingdom_spans(world, kingdom_name, kingdom_hold, kingdom_led_by, kingdom_e, player_e);

    world.entity_mut(body_e).despawn_children();
    world.commands().entity(body_e).with_children(|p| {
        for (text, color) in spans {
            spawn_span(p, text, color);
        }
    });
}

/// Build every visible span for the pinned kingdom. Each sub-renderer is a
/// pure `(text, color)` builder; the caller iterates and spawns.
fn render_kingdom_spans(
    world: &mut World,
    name: String,
    land_e: Option<Entity>,
    ruler_e: Option<Entity>,
    kingdom_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let mut spans: Vec<(String, Color)> = Vec::new();
    spans.push((format!("{}\n", name), TITLE));
    if let Some(land_e) = land_e {
        if let Some(land_name) = world.get::<LandName>(land_e) {
            spans.push((format!("land: {}\n", land_name.0), Color::WHITE));
        }
        spans.extend(render_buildings_spans(world, land_e));
    }
    if let Some(ruler_e) = ruler_e {
        spans.extend(render_ruler_spans(world, ruler_e, player_e));
    }
    spans.extend(render_courtiers_spans(world, kingdom_e, player_e));
    spans.extend(render_wars_spans(world, kingdom_e));
    spans.extend(render_armies_spans(world, kingdom_e));
    spans
}

fn render_ruler_spans(
    world: &mut World,
    ruler_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let ent = world.entity(ruler_e);
    let Some(name) = ent.get::<CharacterName>() else {
        return Vec::new();
    };
    let Some(dob) = ent.get::<CharacterDateOfBirth>() else {
        return Vec::new();
    };
    let Some(gender) = ent.get::<CharacterGender>() else {
        return Vec::new();
    };
    let house = ent
        .get::<CharacterOfHouse>()
        .and_then(|cof| world.entity(cof.0).get::<HouseName>())
        .map(|hn| hn.0.clone())
        .unwrap_or_default();
    let ruler_age = age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
    let marker = match gender {
        CharacterGender::Male => "m",
        CharacterGender::Female => "f",
    };

    let mut spans = vec![
        ("ruler: ".to_string(), Color::WHITE),
        (format!("{} {}", name.0, house), Color::WHITE),
        (format!(" [{}] ({})", marker, ruler_age), Color::WHITE),
    ];
    if let Some(player) = player_e.filter(|p| *p != ruler_e) {
        let date = world.resource::<Date>().clone();
        let op = opinion_of_via_world(world, ruler_e, player, &date);
        spans.push((" [".to_string(), Color::WHITE));
        spans.push((format!("{:+}", op), opinion_color(op)));
        spans.push(("]\n".to_string(), Color::WHITE));
    } else {
        spans.push(("\n".to_string(), Color::WHITE));
    }
    spans
}

/// Inlined `opinion_of` for callers that hold `&mut World` rather than a
/// Bevy `Query` system param. Same rules as `helper::opinion_helper::opinion_of`.
fn opinion_of_via_world(world: &mut World, observer: Entity, target: Entity, today: &Date) -> i32 {
    let mut v: i32 = 0;
    let o_house = world.get::<CharacterOfHouse>(observer).map(|c| c.0);
    let t_house = world.get::<CharacterOfHouse>(target).map(|c| c.0);
    if o_house.is_some() && o_house == t_house {
        v += 10;
    }
    let o_husband = world
        .get::<CharacterHasHusband>(observer)
        .map(|c| c.0);
    let t_husband = world
        .get::<CharacterHasHusband>(target)
        .map(|c| c.0);
    if o_husband == Some(target) || t_husband == Some(observer) {
        v += 50;
    }
    let fo = world.get::<CharacterHasFather>(observer).map(|c| c.0);
    let mo = world.get::<CharacterHasMother>(observer).map(|c| c.0);
    let ft = world.get::<CharacterHasFather>(target).map(|c| c.0);
    let mt = world.get::<CharacterHasMother>(target).map(|c| c.0);
    let parent_child = fo == Some(target)
        || mo == Some(target)
        || ft == Some(observer)
        || mt == Some(observer);
    let sibling = (fo.is_some() && fo == ft) || (mo.is_some() && mo == mt);
    if parent_child || sibling {
        v += 20;
    }
    // Memory contribution — scan once via a fresh QueryState (the helper is
    // called per-ruler/courtier, so a per-call state cache would be heavier).
    let mut mem_q = world.query::<(
        &MemoryOfCharacter,
        &MemoryTowardCharacter,
        &MemoryUntilDate,
        &MemoryKind,
    )>();
    for (of, toward, until, kind) in mem_q.iter(world) {
        if of.0 != observer || toward.0 != target {
            continue;
        }
        if until.0 <= *today {
            continue;
        }
        match kind {
            MemoryKind::ReceivedGold { amount } => v += *amount as i32,
        }
    }
    v
}

fn render_courtiers_spans(
    world: &mut World,
    kingdom_e: Entity,
    player_e: Option<Entity>,
) -> Vec<(String, Color)> {
    let courtiers: Vec<Entity> = world
        .get::<KingdomHasCourtiers>(kingdom_e)
        .map(|k| k.iter().collect())
        .unwrap_or_default();
    if courtiers.is_empty() {
        return Vec::new();
    }
    let mut entries: Vec<(Entity, String, String, u32, &'static str)> = Vec::new();
    let mut court_chars = world.query::<&CourtierOfCharacter>();
    let mut characters =
        world.query::<(&CharacterName, &CharacterDateOfBirth, &CharacterGender)>();
    let mut character_of_house = world.query::<&CharacterOfHouse>();
    let mut houses = world.query::<&HouseName>();
    for courtier_e in courtiers {
        let Some(coc) = court_chars.get(world, courtier_e).ok() else {
            continue;
        };
        let char_e = coc.0;
        let Ok((name, dob, gender)) = characters.get(world, char_e) else {
            continue;
        };
        let house = character_of_house
            .get(world, char_e)
            .ok()
            .and_then(|cof| houses.get(world, cof.0).ok())
            .map(|hn| hn.0.clone())
            .unwrap_or_default();
        let char_age = age(&dob.0, world.resource::<Date>(), world.resource::<Calendar>());
        let marker = match gender {
            CharacterGender::Male => "m",
            CharacterGender::Female => "f",
        };
        entries.push((char_e, name.0.clone(), house, char_age, marker));
    }
    if entries.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("courtiers:\n".to_string(), TITLE)];
    for (i, (char_e, name, house, age, marker)) in entries.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((format!("{} {}", name, house), Color::WHITE));
        spans.push((format!(" [{}] ({})", marker, age), Color::WHITE));
        if let Some(player) = player_e {
            let date = world.resource::<Date>().clone();
            let op = opinion_of_via_world(world, *char_e, player, &date);
            spans.push((" [".to_string(), Color::WHITE));
            spans.push((format!("{:+}", op), opinion_color(op)));
            spans.push(("]".to_string(), Color::WHITE));
        }
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

fn render_wars_spans(world: &mut World, kingdom_e: Entity) -> Vec<(String, Color)> {
    let mut lines: Vec<String> = Vec::new();
    let mut wars = world.query::<(&WarName, &WarBeginDate)>();
    if let Some(attacking) = world.get::<KingdomHasWarsAttacking>(kingdom_e) {
        for war_e in attacking.iter() {
            if let Ok((name, begin)) = wars.get(world, war_e) {
                lines.push(format!("{} ({})", name.0, begin.0));
            }
        }
    }
    if let Some(defending) = world.get::<KingdomHasWarsDefending>(kingdom_e) {
        for war_e in defending.iter() {
            if let Ok((name, begin)) = wars.get(world, war_e) {
                lines.push(format!("[def] {} ({})", name.0, begin.0));
            }
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("wars:\n".to_string(), TITLE)];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((line.clone(), Color::WHITE));
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

fn render_armies_spans(world: &mut World, kingdom_e: Entity) -> Vec<(String, Color)> {
    let armies: Vec<Entity> = world
        .get::<KingdomHasArmies>(kingdom_e)
        .map(|k| k.iter().collect())
        .unwrap_or_default();
    if armies.is_empty() {
        return Vec::new();
    }
    let mut armies_q = world.query::<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        &ArmyMaxLevy,
        Option<&ArmyMarching>,
    )>();
    let mut army_queues_q = world.query::<&ArmyHasMarching>();
    let mut army_sieges_q = world.query::<&ArmyHasSiege>();
    let mut army_marching_q = world.query::<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>();
    let mut siege_q = world.query::<&SiegeProgress>();
    let mut roads_q = world.query::<&RoadDistanceDays>();
    let mut lands_q = world.query::<&LandName>();
    let calendar = world.resource::<Calendar>().clone();
    let date = world.resource::<Date>().clone();
    let mut lines: Vec<String> = Vec::new();
    for army_e in armies {
        if let Some(line) = army_line(
            army_e,
            &mut armies_q,
            &mut army_queues_q,
            &mut army_sieges_q,
            &mut army_marching_q,
            &mut siege_q,
            &mut roads_q,
            &mut lands_q,
            world,
            &calendar,
            &date,
        ) {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("armies:\n".to_string(), TITLE)];
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((line.clone(), Color::WHITE));
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

// ponytail: duplicated from ui/army.rs — the format is one small match arm,
// and the two call sites want exactly the same string today. Pull a shared
// helper into ui/army.rs the moment a third caller appears or the format
// starts branching between the panels.
#[allow(clippy::too_many_arguments)]
fn army_line(
    army_e: Entity,
    armies: &mut bevy::ecs::query::QueryState<(
        &ArmyName,
        &ArmyLevy,
        &ArmyOnLand,
        &ArmyStatus,
        &ArmyMaxLevy,
        Option<&ArmyMarching>,
    )>,
    army_queues: &mut bevy::ecs::query::QueryState<&ArmyHasMarching>,
    army_sieges: &mut bevy::ecs::query::QueryState<&ArmyHasSiege>,
    army_marching: &mut bevy::ecs::query::QueryState<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    siege_progress: &mut bevy::ecs::query::QueryState<&SiegeProgress>,
    roads: &mut bevy::ecs::query::QueryState<&RoadDistanceDays>,
    lands: &mut bevy::ecs::query::QueryState<&LandName>,
    world: &mut World,
    calendar: &Calendar,
    date: &Date,
) -> Option<String> {
    let (name, levy, on_land, status, max_levy, current_marching) =
        armies.get(world, army_e).ok()?;
    let current_land = lands
        .get(world, on_land.0)
        .ok()
        .map(|land_name| land_name.0.clone())
        .unwrap_or_else(|| "?".into());
    let base = format!("{} ({}) at {}", name.0, levy.0, current_land);
    match status {
        ArmyStatus::Idle => Some(base),
        ArmyStatus::Raising => Some(format!("{base} raising {}/{}", levy.0, max_levy.0)),
        ArmyStatus::Marching => {
            let queue = army_queues.get(world, army_e).ok()?;
            let hops: Vec<_> = queue.iter().collect();
            let (final_dest, total_days) = route_summary(
                &hops,
                current_marching.copied().map(|m| m.0),
                army_marching,
                roads,
                lands,
                world,
                calendar,
                date,
            );
            Some(format!("{base} marching to {final_dest} at {total_days} days"))
        }
        ArmyStatus::Sieging => {
            let progress = army_sieges
                .get(world, army_e)
                .ok()
                .and_then(|ahs| siege_progress.get(world, ahs.siege()).ok())
                .map(|sp| sp.0)
                .unwrap_or(0);
            Some(format!("{base} sieging at {progress}%"))
        }
    }
}

fn route_summary(
    hops: &[Entity],
    current_marching: Option<Entity>,
    army_marching: &mut bevy::ecs::query::QueryState<(
        &MarchingStatus,
        &MarchingToLand,
        &MarchingOnRoad,
        Option<&MarchingArrivedDate>,
    )>,
    roads: &mut bevy::ecs::query::QueryState<&RoadDistanceDays>,
    lands: &mut bevy::ecs::query::QueryState<&LandName>,
    world: &World,
    calendar: &Calendar,
    date: &Date,
) -> (String, i64) {
    let today_ord = date.ordinal(calendar);
    let on_route_days: i64 = current_marching
        .and_then(|cur| army_marching.get(world, cur).ok())
        .and_then(|(_, _, _, arrived_opt)| arrived_opt.and_then(|d| d.0))
        .map(|arrived| (arrived.ordinal(calendar) - today_ord).max(0))
        .unwrap_or(0);
    let mut total_days: i64 = 0;
    for &hop in hops {
        if let Ok((_, _, on_road, _)) = army_marching.get(world, hop)
            && let Some(road_distance_days) = roads.get(world, on_road.0).ok()
        {
            total_days += road_distance_days.0 as i64;
        }
    }
    if current_marching.is_some()
        && let Some(cur) = current_marching
        && let Ok((_, _, on_road, _)) = army_marching.get(world, cur)
        && let Some(road_distance_days) = roads.get(world, on_road.0).ok()
    {
        total_days -= road_distance_days.0 as i64;
    }
    total_days += on_route_days;
    let final_dest = hops
        .last()
        .and_then(|&h| army_marching.get(world, h).ok())
        .and_then(|(_, to, _, _)| lands.get(world, to.0).ok())
        .map(|n| n.0.clone())
        .unwrap_or_else(|| "?".into());
    (final_dest, total_days)
}

fn render_buildings_spans(world: &mut World, land_e: Entity) -> Vec<(String, Color)> {
    let buildings: Vec<Entity> = world
        .get::<LandHasBuildings>(land_e)
        .map(|l| l.iter().collect())
        .unwrap_or_default();
    if buildings.is_empty() {
        return Vec::new();
    }

    // Build QueryStates up-front (each takes &mut World momentarily); the
    // BuildingDefs resource is fetched inline with `.cloned()` so it never
    // holds a borrow across a query call.
    let mut of_q = world.query::<&BuildingOf>();
    let mut status_q = world.query::<&BuildingStatus>();
    let mut levy_q = world.query::<&BuildingLevy>();
    let mut raised_q = world.query::<&BuildingIsRaised>();

    struct Row {
        name: String,
        name_color: Color,
        // Spec: profit, max levy, upkeep — each shown when > 0.
        profit: Option<u32>,
        levy: Option<u32>,
        upkeep: Option<u32>,
    }
    let mut rows: Vec<Row> = Vec::new();
    for building_e in buildings {
        let Some(bof) = of_q.get(world, building_e).ok() else { continue };
        let Some(d) = world.resource::<BuildingDefs>().get(&bof.0).cloned() else {
            continue;
        };
        let status = status_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingStatus::Active);
        let is_raised = raised_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingIsRaised(false))
            .0;
        let current_levy = levy_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingLevy(0))
            .0;
        let max_levy = d.levy;

        let (name, name_color) = match status {
            BuildingStatus::Inactive | BuildingStatus::Building => {
                (d.name.clone(), BUILDING_GRAY)
            }
            BuildingStatus::Active => {
                if is_raised {
                    (d.name.clone(), RAISED_RED)
                } else if current_levy < max_levy {
                    (
                        format!("{} ({}/{})", d.name, current_levy, max_levy),
                        PARTIAL_YELLOW,
                    )
                } else {
                    (d.name.clone(), Color::WHITE)
                }
            }
        };
        rows.push(Row {
            name,
            name_color,
            profit: if d.gold_profit > 0 { Some(d.gold_profit) } else { None },
            levy: if d.levy > 0 { Some(d.levy) } else { None },
            upkeep: if d.gold_upkeep > 0 { Some(d.gold_upkeep) } else { None },
        });
    }
    if rows.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("buildings:\n".to_string(), TITLE)];
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((format!("{} ", row.name), row.name_color));
        if let Some(p) = row.profit {
            spans.push((format!("+{}g ", p), GOLD_COLOR));
        }
        if let Some(l) = row.levy {
            spans.push((format!("{} ", l), LEVY_GREEN));
        }
        if let Some(u) = row.upkeep {
            spans.push((format!("-{}g", u), UPKEEP_GRAY));
        }
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

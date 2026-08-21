use anyhow::{Result, bail};
use bevy::prelude::*;
use kings_game::app::{Game, speed};
use kings_game::chronicles;
use kings_game::commands;
use kings_game::ctx::Ctx;
use kings_game::ecs;
use kings_game::resources::chronicle::Chronicles;
use kings_game::resources::input_layer::InputLayer;
use kings_game::schedules::{OnDay, OnMonth};
use kings_game::ui;
use kings_game::ui::command_menu::CommandMenuUiContext;
use kings_game::game;
use kings_game::game::presenting_event::deck_from_state as initial_event_deck;
use kings_game::map::components::army_icon;
use kings_game::map::components::border_graphic;
use kings_game::map::components::holding_icon;
use kings_game::map::components::land_graphic;
use kings_game::map::components::road_graphic;
use std::path::Path;

fn main() -> Result<()> {
    let mut seed = None;
    let mut player = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--player-character-id" => match args.next() {
                Some(id) => player = Some(id),
                None => bail!("--player-character-id needs a character id"),
            },
            _ => seed = arg.parse().ok(),
        }
    }
    let seed = seed.unwrap_or(0xC0FFEE);
    let Some(player) = player else {
        bail!("usage: kings-game [seed] --player-character-id <id>");
    };

    let mods_dir = std::env::var("KINGS_MODS").unwrap_or_else(|_| "mods".into());
    let mods = kings_game::mods::load(Path::new(&mods_dir))?;
    if mods.content.character(&player).is_none() {
        bail!("no character `{player}` in the loaded mods");
    }
    let calendar = mods.content.calendar.clone();
    let border = mods.content.border;
    let start = calendar.start;
    // `Content` is moved into `populate` below; pull out anything we need
    // before that move so we can read it from this scope.
    let event_deck_state = mods.content.event_deck;
    let event_scripts = mods.event_scripts;

    let ctx = Ctx::new_game(seed, &player);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Kings".into(),
                    mode: bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                    ..default()
                }),
                ..default()
            })
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=warn,winit=error,bevy_winit=error".into(),
                ..default()
            }),
    );
    app.insert_gizmo_config(
        land_graphic::LandBorderGizmoConfigGroup,
        GizmoConfig {
            line: GizmoLineConfig { width: 1.0, ..default() },
            ..default()
        },
    );
    app.insert_gizmo_config(
        road_graphic::RoadGizmoConfigGroup,
        GizmoConfig {
            line: GizmoLineConfig {
                width: 2.0,
                style: GizmoLineStyle::Dashed {
                    gap_scale: road_graphic::DASH_GAP_SCALE,
                    line_scale: road_graphic::DASH_LINE_SCALE,
                },
                ..default()
            },
            ..default()
        },
    );
    {
        let world = app.world_mut();
        ecs::populate(world, mods.content);
    }

    let game = Game::new(ctx);
    let hz = f64::from(speed(&calendar.speeds, game.speed_idx));
    app.insert_resource(game)
        .insert_resource(start)
        .insert_resource(Chronicles::default())
        .insert_resource(calendar)
        .insert_resource(border)
        .insert_resource(InputLayer::default())
        .insert_resource(CommandMenuUiContext::default())
        .insert_resource(kings_game::ui::kingdom::KingdomUiContext::default())
        .insert_resource(kings_game::ui::character::CharacterUiContext::default())
        .insert_resource(kings_game::ui::wiki::WikiUiContext::default())
        .insert_resource(kings_game::ui::event_popup::EventPopupUiContext::default())
        .insert_resource(initial_event_deck(&event_deck_state))
        .insert_resource(event_scripts)
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .add_systems(
            Startup,
            (
                Ctx::startup,
                ui::startup::startup,
                ui::camera::startup,
                ui::command_menu::startup,
                ui::error::startup,
                ui::event_popup::startup,
                ui::wiki::startup,
                commands::startup,
                game::yielding::recompute_yields,
                border_graphic::startup,
                holding_icon::startup,
                land_graphic::startup,
                road_graphic::startup,
            ),
        )
        .add_observer(game::yielding::on_building_updated)
        .add_observer(army_icon::on_army_raised)
        .add_observer(army_icon::on_army_dismiss)
        .add_observer(ui::error::on_error_occurred)
        .add_observer(ui::event_popup::on_event_presented)
        .add_observer(ui::event_popup::on_event_resolved)
        .add_observer(chronicles::on_event_resolved)
        .add_observer(chronicles::on_construction_started)
        .add_observer(chronicles::on_constructed)
        .add_observer(chronicles::on_destroyed)
        .add_observer(chronicles::on_army_raised)
        .add_observer(chronicles::on_army_dismiss)
        .add_observer(chronicles::on_marching_ordered)
        .add_observer(chronicles::on_army_arrived)
        .add_observer(chronicles::on_siege_laid)
        .add_observer(chronicles::on_siege_won)
        .add_observer(chronicles::on_war_declared)
        .add_observer(chronicles::on_demand_enforced)
        .add_observer(game::building_releasing::on_demand_enforced)
        .add_observer(game::court_releasing::on_demand_enforced)
        .add_observer(chronicles::on_war_ended)
        .add_observer(chronicles::on_character_died)
        .add_observer(chronicles::on_kingdom_succeeded)
        .add_observer(game::inheriting::on_character_died)
        .add_observer(chronicles::on_gold_gifted)
        .add_observer(game::presenting_event::on_event_resolved)
        .add_systems(
            Update,
            (
                (ui::input::global_keys, ui::input::map_selection)
                    .run_if(ui::input::root_layer_active),
                ui::command_menu::input
                    .run_if(ui::command_menu::command_menu_layer_active),
                ui::wiki::input
                    .run_if(ui::wiki::wiki_layer_active),
                ui::error::input
                    .run_if(ui::error::error_popup_layer_active),
                ui::event_popup::input
                    .run_if(ui::event_popup::input_layer_active),
                ui::event_popup::update
                    .run_if(ui::event_popup::event_popup_layer_active),
                ui::resource::update,
                ui::status::update,
                kings_game::debug::dump_characters,
            ),
        )
        // Split into a second `add_systems` so the kingdom panel is registered
        // past the 16-system tuple ceiling on the first. Cheap two-line add;
        // collapse back into the main tuple if the schedule ever shrinks below 16.
        .add_systems(
            Update,
            (
                ui::kingdom::input.run_if(ui::kingdom::root_layer_active),
                ui::kingdom::update,
                ui::character::input.run_if(ui::input::root_layer_active),
                ui::character::update,
            ),
        )
        .add_systems(
            PostUpdate,
            (
                ui::camera::update_camera,
                border_graphic::update,
                land_graphic::update,
                road_graphic::update,
                holding_icon::update,
                army_icon::update,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            game::advancing_date::tick.run_if(|g: Res<Game>| g.running()),
        )
        .add_systems(
            OnDay,
            (
                game::constructing::on_day,
                game::marching::on_day,
                game::raising_army::on_day,
                game::besieging::on_day,
                game::aging::on_day,
                game::remembering::on_day,
                game::presenting_event::on_day,
            ),
        )
        .add_systems(
            OnMonth,
            (
                game::paying_out::on_month,
                game::replenishing_levy::on_month,
            ),
        )
        .run();
    Ok(())
}

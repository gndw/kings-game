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
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .add_systems(
            Startup,
            (
                Ctx::startup,
                ui::startup::startup,
                ui::camera::startup,
                ui::command_menu::startup,
                ui::error::startup,
                commands::startup,
                game::yields::recompute_yields,
                border_graphic::startup,
                holding_icon::startup,
                land_graphic::startup,
                road_graphic::startup,
            ),
        )
        .add_observer(game::yields::on_building_updated)
        .add_observer(army_icon::on_army_raised)
        .add_observer(army_icon::on_army_dismiss)
        .add_observer(ui::error::on_error_occured)
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
        .add_systems(
            Update,
            (
                (ui::input::global_keys, ui::input::map_selection)
                    .run_if(ui::input::root_layer_active),
                ui::command_menu::input
                    .run_if(ui::command_menu::command_menu_layer_active),
                ui::error::input
                    .run_if(ui::error::error_popup_layer_active),
                ui::courts::update,
                ui::resource::update,
                ui::information::update,
                ui::buildings::update,
                ui::chronicle::update,
                ui::wars::update,
                ui::army::update,
                ui::status::update,
                kings_game::debug::dump_characters,
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
            game::advance_date::advance.run_if(|g: Res<Game>| g.running()),
        )
        .add_systems(
            OnDay,
            (
                game::construction::construction,
                game::marching::tick,
                game::raising_army::on_day,
                game::siege::tick,
            ),
        )
        .add_systems(
            OnMonth,
            (
                game::payout::payout,
                game::replenish_levy::replenish,
            ),
        )
        .run();
    Ok(())
}

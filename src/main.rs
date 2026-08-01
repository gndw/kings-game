use anyhow::Result;
use bevy::prelude::*;
use kings_game::app::{Game, input, tick};
use kings_game::ecs::Ctx;
use kings_game::ui;
use std::path::Path;

fn main() -> Result<()> {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xC0FFEE);

    // KINGS_MAP lets modders point at their own file.
    let map_path = std::env::var("KINGS_MAP").unwrap_or_else(|_| "assets/map.ron".into());
    let map = kings_game::map::load(Path::new(&map_path))?;
    let game = Game::new(Ctx::new_game(seed, map));
    let hz = f64::from(game.speed);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Kings".into(),
                mode: bevy::window::WindowMode::BorderlessFullscreen(
                    MonitorSelection::Current,
                ),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(game)
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .add_systems(Startup, (ui::startup::startup, ui::map::startup))
        .add_systems(
            Update,
            (
                input,
                ui::map::update_input,
                ui::map::update_draw,
                ui::legend::update,
                ui::chronicle::update,
                ui::status::update,
            ),
        )
        .add_systems(FixedUpdate, tick.run_if(|g: Res<Game>| g.running()))
        .run();
    Ok(())
}

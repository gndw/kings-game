use anyhow::Result;
use bevy::prelude::*;
use kings_game::app::{Game, input, tick};
use kings_game::ctx::Ctx;
use kings_game::ui;
use std::path::Path;

fn main() -> Result<()> {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xC0FFEE);

    // KINGS_MODS lets modders point at their own mods directory.
    let mods_dir = std::env::var("KINGS_MODS").unwrap_or_else(|_| "mods".into());
    let mods = kings_game::mods::load(Path::new(&mods_dir))?;
    let game = Game::new(Ctx::new_game(seed, mods.content), mods.scripts);
    let hz = f64::from(game.speed());

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Kings".into(),
                mode: bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current),
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
                ui::resource::update,
                ui::status::update,
            ),
        )
        .add_systems(FixedUpdate, tick.run_if(|g: Res<Game>| g.running()))
        .run();
    Ok(())
}

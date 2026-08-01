use anyhow::Result;
use bevy::prelude::*;
use kings_game::app::{Game, input, tick};
use kings_game::ecs::Ctx;
use kings_game::ui;
use std::path::Path;

/// Parsed command-line arguments.
struct Cli {
    seed: u64,
    load: Option<String>,
    player_character_id: Option<String>,
}

fn parse_args() -> Cli {
    let mut cli = Cli {
        seed: 0xC0FFEE,
        load: None,
        player_character_id: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--load" => cli.load = args.next(),
            "--player-character-id" => cli.player_character_id = args.next(),
            s if s.starts_with("--") => eprintln!("unknown flag: {s}"),
            s => {
                if let Ok(seed) = s.parse() {
                    cli.seed = seed;
                }
            }
        }
    }
    cli
}

fn main() -> Result<()> {
    let cli = parse_args();

    let ctx = if let Some(ref path) = cli.load {
        kings_game::save::Save::load(Path::new(path))?.restore()
    } else {
        // KINGS_MAP lets modders point at their own file.
        let map_path =
            std::env::var("KINGS_MAP").unwrap_or_else(|_| "assets/map.ron".into());
        let map = kings_game::map::load(Path::new(&map_path))?;
        Ctx::new_game(cli.seed, map, cli.player_character_id)
    };

    let game = Game::new(ctx);
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

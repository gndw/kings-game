use anyhow::{Result, bail};
use bevy::prelude::*;
use kings_game::app::{Game, input, speed};
use kings_game::ctx::Ctx;
use kings_game::ecs;
use kings_game::resources::date::Date;
use kings_game::schedules::OnMonth;
use kings_game::ui;
use kings_game::updates;
use std::path::Path;

fn main() -> Result<()> {
    // ponytail: two options and one of them positional, so no arg-parsing
    // crate. `--player-character-id=x` isn't accepted; add it if anyone asks.
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
    // Required: nobody is the obvious character to be, and picking one for you
    // would just be the old hardcoding somewhere less visible.
    let Some(player) = player else {
        bail!("usage: kings-game [seed] --player-character-id <id>");
    };

    // KINGS_MODS lets modders point at their own mods directory.
    let mods_dir = std::env::var("KINGS_MODS").unwrap_or_else(|_| "mods".into());
    let mods = kings_game::mods::load(Path::new(&mods_dir))?;
    // A typo'd id would otherwise start a game as nobody: no capital to open
    // on and a blank resource bar. Say so instead.
    if mods.content.character(&player).is_none() {
        bail!("no character `{player}` in the loaded mods");
    }
    let calendar = mods.content.calendar.clone();
    let border = mods.content.border;

    // Session state without the world; entities are spawned into the App world
    // below, and the opening selection resolves once they exist.
    let mut ctx = Ctx::new_game(seed, &player);

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
            // ponytail: WSLg has no XSETTINGS manager, reports a 0mm display, and
            // can't report its current monitor at window creation. All of these are
            // environment noise, not bugs.
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=warn,winit=error,bevy_winit=error".into(),
                ..default()
            }),
    );
    {
        let world = app.world_mut();
        ecs::populate(world, mods.content, mods.state);
        ctx.finish_selection(world);
    }

    let game = Game::new(ctx);
    let hz = f64::from(speed(&calendar.speeds, game.speed_idx));
    app.insert_resource(game)
        .insert_resource(Date::START)
        .insert_resource(calendar)
        .insert_resource(border)
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .add_systems(
            Startup,
            (
                ui::startup::startup,
                ui::map::startup,
                updates::yields::recompute_yields,
            ),
        )
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
        .add_systems(
            FixedUpdate,
            updates::advance_date::advance.run_if(|g: Res<Game>| g.running()),
        )
        .add_systems(OnMonth, updates::payout::payout)
        .run();
    Ok(())
}

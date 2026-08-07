use anyhow::{Result, bail};
use bevy::prelude::*;
use kings_game::app::{Game, input, speed};
use kings_game::commands::CommandRegistry;
use kings_game::ctx::Ctx;
use kings_game::ecs;
use kings_game::resources::chronicle::Chronicles;
use kings_game::schedules::{OnDay, OnMonth};
use kings_game::ui;
use kings_game::ui::command_menu::CommandMenu;
use kings_game::game;
use kings_game::map;
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
    let start = calendar.start;

    // Session state without the world; entities are spawned into the App world
    // below, and the opening selection resolves once they exist.
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
            // ponytail: WSLg has no XSETTINGS manager, reports a 0mm display, and
            // can't report its current monitor at window creation. All of these are
            // environment noise, not bugs.
            .set(bevy::log::LogPlugin {
                filter: "wgpu=error,naga=warn,winit=error,bevy_winit=error".into(),
                ..default()
            }),
    );
    // Register the per-land-border gizmo group with a thinner 1.0px stroke;
    // `update_draw` draws the polygon outline through `Gizmos<LandBorder...>`
    // because `linestrip_2d` has no per-call width.
    app.insert_gizmo_config(
        ui::map::LandBorderGizmoConfigGroup,
        GizmoConfig {
            line: GizmoLineConfig {
                width: 1.0,
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
        .insert_resource(Chronicles(vec![format!(
            "{start} -- the chronicle begins."
        )]))
        .insert_resource(calendar)
        .insert_resource(border)
        .insert_resource(CommandMenu::default())
        .insert_resource(CommandRegistry::default())
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .add_systems(
            Startup,
            (
                Ctx::startup,
                ui::startup::startup,
                ui::camera::startup,
                ui::map::startup,
                ui::command_menu::startup,
                game::yields::recompute_yields,
            ),
        )
        // The construct / destroy commands (and any future code path that
        // mutates a building's kingdom-graph footprint) trigger
        // `OnBuildingUpdated`. The observer walks
        // `land → LandHeldBy → kingdom → KingdomLedBy → leader` and writes the
        // new yield. `ui::resource::update` sits in `PostUpdate` so its read
        // lands on the same frame as the observer's write.
        .add_observer(game::yields::on_building_updated)
        .add_systems(
            Update,
            (
                input,
                ui::command_menu::input,
                ui::command_menu::update,
                ui::map::update_input,
                ui::actions::update,
                ui::courts::update,
                ui::army::update,
                // Ponytail: keep debug systems last so they don't displace
                // gameplay systems in the schedule.
                ui::resource::update,
                ui::information::update,
                ui::buildings::update,
                ui::chronicle::update,
                ui::status::update,
                kings_game::debug::dump_characters,
            ),
        )
        .add_systems(
            PostUpdate,
            (
                // update_camera mutates Projection/Transform; must run before
                // update_draw so gizmos draw against the new view.
                ui::camera::update_camera,
                ui::map::update_draw,
                map::army::update,
            ),
        )
        .add_systems(
            FixedUpdate,
            // `advance` runs the date; `construction` lives on the `OnDay`
            // schedule `advance` fires, so adding it here would run it twice
            // a day.
            game::advance_date::advance.run_if(|g: Res<Game>| g.running()),
        )
        .add_systems(
            OnDay,
            (
                game::construction::construction,
                game::marching::tick,
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

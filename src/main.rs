use anyhow::{Result, bail};
use bevy::prelude::*;
use kings_game::app::{Game, speed};
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
    // `land_graphic::update` draws the polygon outline through
    // `Gizmos<LandBorder...>` because `linestrip_2d` has no per-call width.
    app.insert_gizmo_config(
        land_graphic::LandBorderGizmoConfigGroup,
        GizmoConfig {
            line: GizmoLineConfig {
                width: 1.0,
                ..default()
            },
            ..default()
        },
    );
    // The road dash style is on the config — `Gizmos` has no per-call style —
    // so all road draws share `Gizmos<RoadGizmoConfigGroup>`.
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
        .insert_resource(Chronicles(vec![format!(
            "{start} -- the chronicle begins."
        )]))
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
                // Spawn the error-popup shell, hidden. Runs alongside
                // the command palette startup so the observer in
                // `ui::error::on_error_occured` can find the body to
                // write into on the first error.
                ui::error::startup,
                // Populates `CommandContext` with every command the
                // palette can surface; must run before the panel opens.
                commands::startup,
                game::yields::recompute_yields,
                border_graphic::startup,
                holding_icon::startup,
                land_graphic::startup,
                road_graphic::startup,
            ),
        )
        // The construct / destroy commands (and any future code path that
        // mutates a building's kingdom-graph footprint) trigger
        // `OnBuildingUpdated`. The observer walks
        // `land → LandHeldBy → kingdom → KingdomLedBy → leader` and writes the
        // new yield. `ui::resource::update` sits in `PostUpdate` so its read
        // lands on the same frame as the observer's write.
        .add_observer(game::yields::on_building_updated)
        // `OnArmyRaised` spawns the icon + label trio at the army's
        // `ArmyOnLand`; `OnArmyDismiss` despawns them. Both run after the
        // structural change settles Bevy's relationship hooks.
        .add_observer(army_icon::on_army_raised)
        .add_observer(army_icon::on_army_dismiss)
        // `OnErrorOccured` shows the error popup, force-closes any open
        // command palette, and flips the input layer to `ErrorPopup`.
        // One observer — the popup owns input until the player dismisses.
        .add_observer(ui::error::on_error_occured)
        .add_systems(
            Update,
            (
                // Root-layer input (global keys, map selection) only runs
                // while the palette is closed; the palette owns every
                // keystroke while the `CommandMenu` layer is active.
                (ui::input::global_keys, ui::input::map_selection)
                    .run_if(ui::input::root_layer_active),
                // Palette input: Esc → close, gated to the command-menu
                // layer via its own run-if so it stays dormant on root.
                ui::command_menu::input
                    .run_if(ui::command_menu::command_menu_layer_active),
                // Error popup: Esc → close + flip back to root, gated to
                // the error-popup layer so it stays dormant elsewhere.
                ui::error::input
                    .run_if(ui::error::error_popup_layer_active),
                ui::courts::update,
                // Ponytail: keep debug systems last so they don't displace
                // gameplay systems in the schedule.
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
                // update_camera mutates Projection/Transform; must run before
                // update_draw so gizmos draw against the new view. `.chain()`
                // pins the order explicitly: camera → border → land → road →
                // holding → army, so the road sits over the land fill but
                // under the castle / sword.
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

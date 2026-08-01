//! Input handling and the bits of state that belong to the session rather than
//! the simulation (pause, speed).

use crate::ecs::Ctx;
use bevy::prelude::*;

// ponytail: F5/F9 are the universal quicksave/quickload keys — same as Half-Life, Skyrim, every emulator. Muscle memory is UI.

#[derive(Resource)]
pub struct Game {
    pub ctx: Ctx,
    pub paused: bool,
    /// Simulated days per real second.
    pub speed: u32,
}

impl Game {
    pub fn new(ctx: Ctx) -> Self {
        Game {
            ctx,
            paused: true,
            speed: 8,
        }
    }

    /// True while the sim should keep running on its own.
    pub fn running(&self) -> bool {
        !self.paused
    }
}

/// One simulated day. Runs in `FixedUpdate`, so `Time<Fixed>`'s timestep is the
/// game speed and Bevy owns the clock.
pub fn tick(mut game: ResMut<Game>) {
    game.ctx.tick();
}

pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<Game>,
    mut fixed: ResMut<Time<Fixed>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keys.just_pressed(KeyCode::Space) {
        game.paused = !game.paused;
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        game.speed = (game.speed * 2).min(64);
    }
    if keys.just_pressed(KeyCode::Minus) {
        game.speed = (game.speed / 2).max(1);
    }
    if keys.just_pressed(KeyCode::F5) {
        match crate::save::quicksave(&game.ctx) {
            Ok(path) => game.ctx.chronicles.push(format!(
                "{} — game saved to {}.",
                game.ctx.date,
                path.display()
            )),
            Err(e) => game.ctx.chronicles.push(format!(
                "{} — save failed: {e}.",
                game.ctx.date
            )),
        }
    }
    if keys.just_pressed(KeyCode::F9) {
        match crate::save::quickload() {
            Ok(save) => {
                game.ctx = save.restore();
            }
            Err(e) => game.ctx.chronicles.push(format!(
                "{} — load failed: {e}.",
                game.ctx.date
            )),
        }
    }
    fixed.set_timestep_hz(f64::from(game.speed));
}

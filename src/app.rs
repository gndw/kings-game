//! Input handling and the bits of state that belong to the session rather than
//! the simulation (pause, speed).

use crate::ecs::Ctx;
use bevy::prelude::*;

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
    fixed.set_timestep_hz(f64::from(game.speed));
}

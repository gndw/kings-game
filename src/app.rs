//! Input handling and the bits of state that belong to the session rather than
//! the simulation (pause, speed).

use crate::ctx::Ctx;
use crate::mods::Scripts;
use bevy::prelude::*;

#[derive(Resource)]
pub struct Game {
    pub ctx: Ctx,
    /// Mod scripts. Kept here rather than on `Ctx` so `Ctx` stays pure
    /// simulation state — which is what tests and any future save file want.
    pub scripts: Scripts,
    pub paused: bool,
    /// Which of `ctx.content.speeds` is selected — an index, because the rates
    /// themselves are mod data.
    pub speed_idx: usize,
}

impl Game {
    pub fn new(ctx: Ctx, scripts: Scripts) -> Self {
        Game {
            ctx,
            scripts,
            paused: true,
            speed_idx: 0,
        }
    }

    /// Simulated days per real second. Falls back to 1 rather than panicking on
    /// an empty list; `content::validate` rejects that before a game ever starts.
    pub fn speed(&self) -> u32 {
        self.ctx
            .content
            .speeds
            .get(self.speed_idx)
            .copied()
            .unwrap_or(1)
    }

    /// True while the sim should keep running on its own.
    pub fn running(&self) -> bool {
        !self.paused
    }
}

/// One simulated day, then the mod hooks for it. Runs in `FixedUpdate`, so
/// `Time<Fixed>`'s timestep is the game speed and Bevy owns the clock.
pub fn tick(mut game: ResMut<Game>) {
    let game = &mut *game;
    game.ctx.tick();
    game.scripts.run(&mut game.ctx);
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
    // Step through the mod's speed list rather than doubling: the steps are
    // whatever the data says, and the ends just clamp.
    let last = game.ctx.content.speeds.len().saturating_sub(1);
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        game.speed_idx = (game.speed_idx + 1).min(last);
    }
    if keys.just_pressed(KeyCode::Minus) {
        game.speed_idx = game.speed_idx.saturating_sub(1);
    }
    fixed.set_timestep_hz(f64::from(game.speed()));
}

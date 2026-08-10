//! Input handling and the bits of state that belong to the session rather than
//! the simulation (pause, speed).

use crate::ctx::Ctx;
use crate::resources::calendar::Calendar;
use crate::ui::command_menu::CommandMenu;
use bevy::prelude::*;

#[derive(Resource)]
pub struct Game {
    pub ctx: Ctx,
    pub paused: bool,
    /// Which of the calendar's `speeds` is selected — an index, because the
    /// rates themselves are mod data.
    pub speed_idx: usize,
    /// Camera mode: `false` shows the whole map (the default view), `true`
    /// frames on the selected land's polygon with margin. Toggled by `Z`;
    /// `ui::map::update_camera` reads this every frame.
    pub zoomed: bool,
}

impl Game {
    pub fn new(ctx: Ctx) -> Self {
        Game {
            ctx,
            paused: true,
            speed_idx: 0,
            zoomed: false,
        }
    }

    /// True while the sim should keep running on its own.
    pub fn running(&self) -> bool {
        !self.paused
    }
}

/// Simulated days per real second at `idx` into the calendar's speed list.
/// Falls back to 1 rather than panicking on an empty list; `Calendar::validate`
/// rejects that before a game ever starts.
pub fn speed(speeds: &[u32], idx: usize) -> u32 {
    speeds.get(idx).copied().unwrap_or(1)
}

pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Res<CommandMenu>,
    mut game: ResMut<Game>,
    calendar: Res<Calendar>,
    mut fixed: ResMut<Time<Fixed>>,
    mut exit: MessageWriter<AppExit>,
) {
    // Escape closes the command palette while it's open, so it mustn't quit.
    if !menu.open && (keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Escape)) {
        exit.write(AppExit::Success);
    }
    // Z toggles zoom-to-selection. Yielded to the palette while it's open,
    // same as Esc, so a modal can't lose its keys.
    if !menu.open && keys.just_pressed(KeyCode::KeyZ) {
        game.zoomed = !game.zoomed;
    }
    // Space toggles pause, but only when the palette isn't open — the
    // palette's search bar eats the keystroke so multi-word queries work.
    if !menu.open && keys.just_pressed(KeyCode::Space) {
        game.paused = !game.paused;
    }
    // Digits 1–4 jump straight to a speed and unpause, faster than stepping.
    // The index clamps if a mod lists fewer than four speeds.
    let last = calendar.speeds.len().saturating_sub(1);
    for (key, idx) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
    ] {
        if keys.just_pressed(key) {
            game.speed_idx = idx.min(last);
            game.paused = false;
        }
    }
    fixed.set_timestep_hz(f64::from(speed(&calendar.speeds, game.speed_idx)));
}

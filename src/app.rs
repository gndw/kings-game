//! Session state that doesn't live on entities: pause, sim speed, and the
//! camera mode. The root-layer input handlers (global keys, map selection)
//! live in [`crate::ui::input`].

use crate::ctx::Ctx;
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
    /// `ui::camera::update_camera` reads this every frame.
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

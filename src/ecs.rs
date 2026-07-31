//! The simulation context: the hecs world plus everything that isn't an entity.

use crate::rng::SimRng;
use hecs::World;
use std::sync::{Arc, Mutex};

/// ponytail: 30-day months, 360-day years. Real calendars buy nothing here and
/// cost every date calculation in the game.
pub const DAYS_PER_MONTH: u32 = 30;
pub const DAYS_PER_YEAR: u32 = 360;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub fn advance(&mut self) {
        self.day += 1;
        if u32::from(self.day) > DAYS_PER_MONTH {
            self.day = 1;
            self.month += 1;
            if u32::from(self.month) > DAYS_PER_YEAR / DAYS_PER_MONTH {
                self.month = 1;
                self.year += 1;
            }
        }
    }

    pub fn is_month_start(&self) -> bool {
        self.day == 1
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}.{:02}.{:02}", self.year, self.month, self.day)
    }
}

pub struct Ctx {
    pub world: World,
    pub date: Date,
    pub seed: u64,
    pub tick_count: u64,
    pub rng: Arc<Mutex<SimRng>>,
    pub chronicles: Vec<String>,
    pub selected_region: Option<String>,
}

impl Ctx {
    pub fn new_game(seed: u64) -> Self {
        let mut ctx = Ctx {
            world: World::new(),
            date: Date {
                year: 1066,
                month: 1,
                day: 1,
            },
            seed,
            tick_count: 0,
            rng: Arc::new(Mutex::new(SimRng::new(seed))),
            chronicles: Vec::new(),
            selected_region: None,
        };
        ctx.chronicles
            .push(format!("{} — the chronicle begins.", ctx.date));
        ctx
    }

    /// One simulated day. Systems hook in here.
    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.date.advance();
    }
}

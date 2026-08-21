//! The character panel: a right-docked panel that *replaces* the kingdom
//! panel while the player drills into a character. Opened with **R** while
//! the kingdom panel is pinned (resolves to the kingdom's ruler); **Enter**
//! closes both panels, **Backspace** pops back to the still-pinned kingdom
//! panel.
//!
//! Rendered sections (one line each, matching the kingdom panel style):
//! `name house [gender] (age) [opinion]`, `ruler of: <kingdom>` (when the
//! character leads a kingdom), `gold`, `gold/m`, `levy`, and the six
//! skills. Opinion is suppressed when the character is the player.

mod character;
mod character_detail;
mod character_skills;
mod character_stats;

pub use character::*;

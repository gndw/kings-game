//! Player commands: the one mutation path for player actions.
//!
//! A [`Command`] is self-describing — it owns its rules, its UI steps, and its
//! effect — and [`CommandRegistry`] holds the roster. The command palette
//! ([`crate::ui::command_menu`]) drives *any* registered command's steps the
//! same way, so adding one is a new struct + a
//! [`register`](CommandRegistry::register) line, not edits to the palette. The
//! actor (a character id) is *who* and goes to [`Command::execute`], so the same
//! path serves the player now and AI / networked peers later.
//!
//! - [`core`] — the [`Command`] trait, [`CommandRegistry`], and the shared
//!   id/chronicle/ruled-lands helpers.
//! - [`construct_building`] — build a building kind on a ruled land.
//! - [`destroy_building`] — tear down a building on a ruled land.
//! - [`raise_army`] — raise an army on a ruled land.
//! - [`dismiss_army`] — dismiss an army the actor rules.
//! - [`marching`] — queue a marching order to move an army to another land.
//! - [`declare_war`] — declare war on another kingdom under a casus belli.
//! - [`lay_siege`] — lay siege to a land with one of the player's armies.

pub mod construct_building;
pub mod core;
pub mod declare_war;
pub mod destroy_building;
pub mod dismiss_army;
pub mod lay_siege;
pub mod marching;
pub mod raise_army;

pub use core::{rules_land, Choice, Command, CommandRegistry, MenuItem};

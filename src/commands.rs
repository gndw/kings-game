//! Player commands: the one mutation path for player actions.
//!
//! A [`Command`] is *what to do*; the actor (a character id) is *who* and is
//! passed to [`apply`], so the same path serves the player now and AI/networked
//! peers later. Input builds a command from keys + the selection and routes it
//! through [`apply`] — an exclusive `&mut World` free function in the style of
//! [`crate::ctx::step`] — so every player mutation flows through one place:
//! validate, apply, chronicle.
//!
//! - [`core`] — the [`Command`] enum, [`apply`] dispatch, and the shared
//!   id/chronicle helpers. (The palette that builds a command from input lives
//!   in [`crate::ui::command_menu`].)
//! - [`construct_building`] — the construct-building command.
//!
//! Extending: add a [`Command`] variant + an arm in [`apply`] + a submodule per
//! command. No trait, no registry — those earn their keep only when modders add
//! commands at runtime, which the compiled game does not.

pub mod construct_building;
pub mod core;

pub use core::{Command, apply};

//! Player commands: the one mutation path for player actions.
//!
//! Each command is a struct that owns its own logic; the shared helpers
//! (menu types, levy pool ops, id generation, ruled-lands walk) live in
//! [`core`]; the per-command logic is in each submodule. The
//! [`BaseCommand`](core::BaseCommand) trait is the surface the palette
//! drives every command through, and
//! [`spawn_command`](core::spawn_command) is the orchestrator the panel
//! calls to populate itself.
//!
//! - [`core`] — shared helpers, the [`BaseCommand`](core::BaseCommand)
//!   trait, and the [`spawn_command`](core::spawn_command) orchestrator.
//! - [`construct_building`] — build a building kind on a ruled land.
//! - [`destroy_building`] — tear down a building on a ruled land.
//! - [`raise_army`] — raise an army on a ruled land.
//! - [`dismiss_army`] — dismiss an army the actor rules.
//! - [`marching`] — queue a marching order to move an army to another land.
//! - [`declare_war`] — declare war on another kingdom under a casus belli.
//! - [`lay_siege`] — lay siege to a land with one of the player's armies.
//! - [`enforce_demands`] — resolve one demand on a war the player is fighting.

pub mod construct_building;
pub mod core;
pub mod declare_war;
pub mod destroy_building;
pub mod dismiss_army;
pub mod enforce_demands;
pub mod lay_siege;
pub mod marching;
pub mod raise_army;

pub use core::{
    spawn_command, startup, update, BaseCommand, CommandContext,
};

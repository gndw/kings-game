//! Which input "layer" the game is currently on.
//!
//! The root layer is the default — map selection, sim pause, zoom, quit all
//! work there. The command-menu layer takes over while the palette is up;
//! the palette's own [`input`](crate::ui::command_menu::input) consumes
//! keystrokes while that layer is active, and the root-layer systems in
//! [`crate::ui::input`] are gated to skip.
//!
//! Transitions are owned by [`crate::ui::command_menu`]: opening the menu
//! moves to [`InputLayer::CommandMenu`], closing it returns to
//! [`InputLayer::Root`].

use bevy::prelude::*;

#[derive(Resource, Default, Eq, PartialEq, Copy, Clone, Debug)]
pub enum InputLayer {
    #[default]
    Root = 1,
    CommandMenu = 2,
}

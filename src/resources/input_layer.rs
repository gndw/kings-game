//! Which input "layer" the game is currently on.
//!
//! The root layer is the default — map selection, sim pause, zoom, quit all
//! work there. The command-menu layer takes over while the palette is up,
//! so the root-layer systems in [`crate::ui::input`] can be gated to skip.
//!
//! Whatever owns the palette flips the layer on open / close.

use bevy::prelude::*;

#[derive(Resource, Default, Eq, PartialEq, Copy, Clone, Debug)]
pub enum InputLayer {
    #[default]
    Root = 1,
    CommandMenu = 2,
}

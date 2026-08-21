//! The kingdom panel: a right-docked panel the player opens with **Enter** to
//! pin a kingdom. Stays pinned as the map selection moves; Enter on a
//! different kingdom switches the pinned kingdom, and Enter on the pinned
//! kingdom closes the panel.
//!
//! Rendered sections: kingdom name, land, ruler, courtiers, wars, armies,
//! buildings. Building row colors match the spec: red when the levy is raised,
//! yellow when the levy is below max, gold for profit, green for max levy,
//! gray for upkeep.

mod kingdom;
mod kingdom_army;
mod kingdom_buildings;
mod kingdom_courts;
mod kingdom_detail;
mod kingdom_war;

pub use kingdom::*;

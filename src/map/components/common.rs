//! Common icon components shared across the map's icon kinds. Visual-only —
//! placement and lifecycle are the icon's job.

use bevy::prelude::*;

/// Back-reference from an icon to the entity it represents. A per-frame
/// `update` system reads the target entity's position component through this
/// and copies the resulting position into the icon's `Transform`. Designed
/// to be generic — any icon that follows an entity (army, kingdom,
/// future character-on-land) can reuse it.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithArmy(pub Entity);

/// Back-reference from an icon to the kingdom it represents. Same shape as
/// [`UIWithArmy`]; kept separate so a single entity can wear multiple
/// `UIWith*` refs (an icon that follows both a kingdom and one of its
/// holdings, for example) without colliding on the field name.
#[derive(Component, Debug, Clone, Copy)]
pub struct UIWithKingdom(pub Entity);

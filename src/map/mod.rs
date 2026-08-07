//! World-space UI: things drawn directly on the map (the camera frame).
//!
//! Currently just the on-map army indicator; the gizmo map drawing itself
//! lives in `crate::ui::map` because it shares the camera + flex layout with
//! the text panels, not because it conceptually belongs to the right-hand
//! column.

pub mod army;

/// On-map label font size. Mirrors `crate::ui::FONT_SIZE`; duplicated rather
/// than shared because both are 1-line constants and the two module trees
/// don't have a common parent to host them.
pub(crate) const FONT_SIZE: f32 = 18.0;
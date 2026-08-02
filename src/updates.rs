//! Simulation systems scheduled by the ECS. The economy lives here now,
//! instead of being called by hand out of `Ctx::tick`.

pub mod payout;
pub mod tick;
pub mod yields;

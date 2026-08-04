//! Command dispatch and the shared helpers every command reaches for (a fresh
//! id, a chronicle line). The input path that *builds* a command lives in
//! [`crate::ui::command_menu`].
//!
//! [`apply`] is an exclusive `&mut World` free function in the style of
//! [`crate::ctx::step`] — it mixes component mutation with resource reads, the
//! case `&mut World` (phased access) handles cleanly.

use super::construct_building;
use crate::app::Game;
use crate::resources::chronicle::Chronicles;
use bevy::ecs::world::World;
use rand::TryRng;

/// A player action: *what* to do. The *who* (a character id) is passed to
/// [`apply`].
pub enum Command {
    /// Build `def_id` on `land_id`, paid for by the acting ruler.
    ConstructBuilding { land_id: String, def_id: String },
}

/// Apply `cmd` for `actor_id`: validate against the rules, mutate, and append a
/// chronicle line on success and every rejection. Exclusive for the same
/// reasons [`crate::ctx::step`] is.
pub fn apply(world: &mut World, actor_id: &str, cmd: Command) {
    match cmd {
        Command::ConstructBuilding { land_id, def_id } => {
            construct_building::construct_building(world, actor_id, &land_id, &def_id)
        }
    }
}

/// A fresh v4 UUID for a runtime-built entity, drawn from the seeded `SimRng`.
///
/// ponytail: the id is generated from `SimRng`, not OS entropy, so it keeps the
/// codebase's one-entropy-source invariant (every bit routed through
/// `try_next_u64`). It is a valid v4 UUID string and unique, but deterministic
/// across replays — which is what this sim wants. Format only, no `uuid` crate,
/// no new dependency.
pub(super) fn next_id(world: &mut World) -> String {
    let rng = world.resource::<Game>().ctx.rng.clone();
    let mut b = [0u8; 16];
    {
        let mut r = rng.lock().unwrap();
        let _ = r.try_fill_bytes(&mut b);
    }
    // v4: version nibble 4, variant 10xx.
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15],
    )
}

/// Append `line` to the chronicle.
pub(super) fn note(world: &mut World, line: String) {
    world.resource_mut::<Chronicles>().0.push(line);
}

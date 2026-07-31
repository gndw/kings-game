//! Seeded RNG that can be restored exactly from a savegame.
//!
//! `StdRng` has no serde support, so instead of storing its state we store the
//! seed plus a draw count and fast-forward on load. Every bit of entropy is
//! routed through `try_next_u64` so that one counter is exact.
//!
//! ponytail: replay costs one `next_u64` per historical draw — a few thousand
//! for a long campaign, i.e. microseconds. If a system ever draws per-entity
//! per-day, swap in `rand_chacha`'s `set_word_pos` instead.

use core::convert::Infallible;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, TryRng};

pub struct SimRng {
    inner: StdRng,
    pub draws: u64,
}

impl SimRng {
    pub fn new(seed: u64) -> Self {
        SimRng {
            inner: StdRng::seed_from_u64(seed),
            draws: 0,
        }
    }

    pub fn restore(seed: u64, draws: u64) -> Self {
        let mut rng = SimRng::new(seed);
        for _ in 0..draws {
            rng.inner.next_u64();
        }
        rng.draws = draws;
        rng
    }
}

impl TryRng for SimRng {
    type Error = Infallible;

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        self.draws += 1;
        Ok(self.inner.next_u64())
    }

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok(self.try_next_u64()? as u32)
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        for chunk in dst.chunks_mut(8) {
            let word = self.try_next_u64()?.to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        Ok(())
    }
}

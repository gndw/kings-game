//! The two things the core still promises: the clock advances correctly and the
//! same seed replays the same draws.

use kings_game::ecs::{Ctx, DAYS_PER_YEAR, Date};
use kings_game::rng::SimRng;
use rand::RngExt;

#[test]
fn a_year_of_ticks_lands_on_the_same_day_next_year() {
    let mut ctx = Ctx::new_game(1, Default::default(), None);
    let start = ctx.date;
    for _ in 0..DAYS_PER_YEAR {
        ctx.tick();
    }
    assert_eq!(
        ctx.date,
        Date {
            year: start.year + 1,
            ..start
        }
    );
    assert_eq!(ctx.tick_count, u64::from(DAYS_PER_YEAR));
}

#[test]
fn a_restored_rng_continues_the_same_sequence() {
    let mut a = SimRng::new(7);
    let skipped: Vec<u32> = (0..5).map(|_| a.random_range(0..1000)).collect();
    let next: Vec<u32> = (0..3).map(|_| a.random_range(0..1000)).collect();

    let mut b = SimRng::restore(7, a.draws - 3);
    assert_eq!(skipped.len(), 5);
    assert_eq!(
        (0..3)
            .map(|_| b.random_range(0..1000))
            .collect::<Vec<u32>>(),
        next
    );
}

// Runs once per simulated day, for every character in the game — the player
// has no special rules, only a different id.
//
// A ruler's levy is whatever their holdings can raise, so it tracks any
// building they gain or lose. Recomputed daily rather than on change: there is
// no "on change" yet, and a few dozen sums a day costs nothing.
//
// Characters who lead no kingdom have no holdings, so `levy_total` is zero for
// them and they raise nothing. That is the whole of "only rulers benefit" —
// there is no separate check for it.

fn on_day(ctx) {
    for id in ctx.characters {
        ctx.set_levy(id, ctx.levy_total(id));
    }
}

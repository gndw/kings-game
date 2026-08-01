// The economy, for every character in the game — the player has no special
// rules, only a different id. Both rules live here rather than in Rust so a mod
// can replace them: ship a folder sorted after `base` with its own mod.rhai, or
// drop this file from your copy entirely.
//
// Characters who lead no kingdom have no holdings, so their totals are zero and
// they neither raise troops nor collect anything. That is the whole of "only
// rulers benefit" — there is no separate check for it.

// A ruler's levy is whatever their holdings can raise, so it tracks any
// building they gain or lose. Recomputed daily rather than on change — there is
// no "on change" yet, and a few dozen sums a day costs nothing.
fn on_day(ctx) {
    for id in ctx.characters {
        ctx.set_levy(id, ctx.levy_total(id));
    }
}

// Taxes are collected on the first of every month. `on_month` only fires on
// day 1, so the date check is already done.
//
// Profit only, no upkeep — soldiers are free for now. A mod wanting realms that
// can bankrupt themselves uses `ctx.gold_profit(id) - ctx.gold_upkeep(id)`.
fn on_month(ctx) {
    for id in ctx.characters {
        let income = ctx.gold_profit(id);
        if income <= 0 { continue; }
        ctx.add_gold(id, income);
        // Only the player's own finances are worth a chronicle line;
        // everyone else's would drown it.
        if id == ctx.player {
            ctx.chronicle("The realm renders " + income + " gold in taxes.");
        }
    }
}

// Runs on the first of every month — `on_month` only fires on day 1, so the
// date check is already done.
//
// Profit only, no upkeep: soldiers are free for now. A mod wanting realms that
// can bankrupt themselves uses `ctx.gold_profit(id) - ctx.gold_upkeep(id)`.
//
// As in on_day.rhai, a character who leads no kingdom has a zero yield and so
// collects nothing, without needing a check for it.

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

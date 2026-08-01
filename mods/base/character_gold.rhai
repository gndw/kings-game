// Taxes are paid on the first of every month — `on_month` only fires on day 1,
// so the date check is already done. The daily hook only publishes what the
// next payout will be, for the UI to show; the sum is the same one either way,
// so what the bar promises is what the month renders.
//
// Profit only, no upkeep: soldiers are free for now. A mod wanting realms that
// can bankrupt themselves subtracts `ctx.building_gold_upkeep(building)` in the
// same loop.
//
// As in character_levy.rhai, the "only rulers earn" rule is the sum itself: a
// character who leads no kingdom holds no buildings and so collects nothing.

// Publish the profit before the first day, never the taxes: a new game opens
// showing what the realm will render, not having already been paid for a month
// nobody played.
fn on_startup(ctx) {
    on_day(ctx);
}

fn on_day(ctx) {
    for id in ctx.characters {
        ctx.set_character_gold_yield(id, realm_income(ctx, id));
    }
}

fn on_month(ctx) {
    for id in ctx.characters {
        let income = realm_income(ctx, id);
        if income <= 0 { continue; }
        ctx.add_character_gold(id, income);
        // Only the player's own finances are worth a chronicle line;
        // everyone else's would drown it.
        if id == ctx.player {
            ctx.add_chronicle("The realm renders " + income + " gold in taxes.");
        }
    }
}

fn realm_income(ctx, who) {
    let total = 0;
    for kingdom in ctx.kingdoms {
        if ctx.kingdom_leader(kingdom) != who { continue; }
        for land in ctx.kingdom_lands(kingdom) {
            for building in ctx.land_buildings(land) {
                total += ctx.building_gold_profit(building);
            }
        }
    }
    total
}

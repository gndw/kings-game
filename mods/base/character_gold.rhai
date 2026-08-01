// Taxes are paid on the first of every month — `on_month` only fires on day 1,
// so the date check is already done. The daily hook only publishes what the
// next payout will be, for the UI to show; the sum is the same one either way,
// so what the bar promises is what the month renders.
//
// Profit minus upkeep, so garrisons cost what they cost — and a realm that owes
// more than it earns is billed the difference. Treasuries are signed and there
// is no floor: a ruler who keeps an army they can't afford goes into debt and
// stays there until they lose the buildings or take someone else's land.
//
// As in character_levy.rhai, the "only rulers earn" rule is the sum itself: a
// character who leads no kingdom holds no buildings and so collects nothing.

// Publish the profit before the first day, never the taxes: a new game opens
// showing what the realm will render, not having already been paid for a month
// nobody played.
fn on_startup(ctx) {
    calculate_character_gold_yield(ctx);
}

fn on_day(ctx) {
    calculate_character_gold_yield(ctx);
}

fn calculate_character_gold_yield(ctx) {
    for id in ctx.characters {
        ctx.set_character_gold_yield(id, get_realm_gold_yield(ctx, id));
    }
}

fn on_month(ctx) {
    for id in ctx.characters {
        let income = get_realm_gold_yield(ctx, id);
        if income == 0 { continue; }
        ctx.add_character_gold(id, income);
        // Only the player's own finances are worth a chronicle line;
        // everyone else's would drown it.
        if id == ctx.player {
            if income > 0 {
                ctx.add_chronicle("The realm renders " + income + " gold in taxes.");
            } else {
                ctx.add_chronicle("The realm's upkeep runs " + (-income) + " gold short.");
            }
        }
    }
}

fn get_realm_gold_yield(ctx, who) {
    let total = 0;
    for kingdom in ctx.kingdoms {
        if ctx.kingdom_leader(kingdom) != who { continue; }
        for land in ctx.kingdom_lands(kingdom) {
            for building in ctx.land_buildings(land) {
                total += ctx.building_gold_profit(building);
                total -= ctx.building_gold_upkeep(building);
            }
        }
    }
    total
}

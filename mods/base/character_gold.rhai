// Runs on the first of every month — `on_month` only fires on day 1, so the
// date check is already done.
//
// Profit only, no upkeep: soldiers are free for now. A mod wanting realms that
// can bankrupt themselves subtracts `ctx.building_gold_upkeep(building)` in the
// same loop.
//
// As in character_levy.rhai, the "only rulers earn" rule is the sum itself: a
// character who leads no kingdom holds no buildings and so collects nothing.

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

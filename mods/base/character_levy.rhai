// Runs once per simulated day, for every character in the game — the player
// has no special rules, only a different id.
//
// A ruler's levy is whatever their holdings can raise, so it tracks any
// building they gain or lose. Recomputed daily rather than on change: there is
// no "on change" yet, and a few dozen sums a day costs nothing.
//
// The rule that only rulers raise troops lives right here in `realm_levy`: a
// character who leads no kingdom matches no realm, so their sum stays zero. A
// mod wanting landless retinues rewrites this loop and nothing else.

fn on_day(ctx) {
    for id in ctx.characters {
        ctx.set_character_levy(id, realm_levy(ctx, id));
    }
}

fn realm_levy(ctx, who) {
    let total = 0;
    for kingdom in ctx.kingdoms {
        if ctx.kingdom_leader(kingdom) != who { continue; }
        for land in ctx.kingdom_lands(kingdom) {
            for building in ctx.land_buildings(land) {
                total += ctx.building_levy(building);
            }
        }
    }
    total
}

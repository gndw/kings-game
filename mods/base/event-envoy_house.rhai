// An envoy from a foreign great house. Weight 2. Three choices: lend gold
// in good faith, accept their tribute, or turn them away. Narration
// references the picked envoy via `{0.name}`.

fn title() {
    "An envoy from a foreign house"
}

fn narration() {
    "An envoy in the colours of another great house bows low in your hall. \
They present the greetings of {0.name}, their lord or lady, and put forward a \
small request on their master's behalf."
}

fn weight() { 2 }

fn choices() {
    [
        #{ text: "Lend them 30 gold in good faith", chronicle: None },
        #{ text: "Accept their 25 gold tribute",    chronicle: None },
        #{ text: "Turn them away",                  chronicle: Some("The envoy returned to {0.name}'s court without an answer.") },
    ]
}

fn decline() {
    "The envoy returned to {0.name}'s court without an answer."
}

fn can_trigger(_world) { true }

fn characters(world) {
    let p = world.player;
    // First cut: any other character. A future pass adds a
    // `character_leads_kingdom(c)` ScriptCtx method so modders can filter
    // tighter (e.g. only house leaders).
    let pool = world.ctx.alive_characters(p)
        .filter(|c| c.house_id != p.house_id);
    if pool.len() == 0 { return []; }
    let idx = world.ctx.rng(0, pool.len() - 1);
    [pool[idx]]
}

fn effect(world) {
    if world.choice_idx == 0 {
        world.ctx.transfer_gold(world.player, world.characters[0], 30);
    } else if world.choice_idx == 1 {
        world.ctx.transfer_gold(world.characters[0], world.player, 25);
    }
}

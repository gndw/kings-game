// A wayfaring stranger at the gates. Weight 3 — the most common popup.
// The script picks the attendee itself (any minor other-house character) via
// `world.ctx.rng`; the runtime uses the returned array as-is. Narration and
// chronicle templates use `{0.name}` to reference the picked character.

fn title() {
    "A wayfaring stranger at your gates"
}

fn narration() {
    "A weary traveller in plain clothes hails you from below the battlements. \
They introduce themselves as {0.name}, and quietly ask whether the court might \
spare a few coins for a hot meal and a bed."
}

fn weight() { 3 }

fn choices() {
    [
        #{ text: "Give 10 gold",   chronicle: None },
        #{ text: "Send them away", chronicle: Some("You turned {0.name} away at the gates.") },
    ]
}

fn decline() {
    "You turned {0.name} away at the gates."
}

fn can_trigger(_world) { true }

fn characters(world) {
    let p = world.player;
    let alive = world.ctx.alive_characters(p);
    let is_other_house = |c| c.house_id != p.house_id;
    let minor_pool = alive.filter(|c| is_other_house(c) && c.levy == 0);
    let candidates = if minor_pool.len() == 0 {
        alive.filter(is_other_house)
    } else {
        minor_pool
    };
    if candidates.len() == 0 { return []; }
    let idx = world.ctx.rng(0, candidates.len() - 1);
    [candidates[idx]]
}

fn effect(world) {
    if world.choice_idx == 0 {
        world.ctx.transfer_gold(world.player, world.characters[0], 10);
    }
}

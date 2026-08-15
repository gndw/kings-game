// A foreign knight seeking service. Weight 1 — the rarest popup. Three
// choices: welcome at 50 gold, welcome at 10 gold, or send away. Attendee
// is a levy-bearing character from another house.

fn title() {
    "A foreign knight seeks service"
}

fn narration() {
    "An armoured figure kneels in your court. They give their name as \
{0.name} — an old warrior whose sword arm is still sound. They ask only for \
coin enough to settle the debts they leave behind before swearing to your \
banner."
}

fn weight() { 1 }

fn choices() {
    [
        #{ text: "Welcome them — 50 gold", chronicle: None },
        #{ text: "Welcome them — 10 gold", chronicle: None },
        #{ text: "Send them away",         chronicle: Some("You turned {0.name} away, and they rode off into the dusk.") },
    ]
}

fn decline() {
    "You turned {0.name} away, and they rode off into the dusk."
}

fn can_trigger(_world) { true }

fn characters(world) {
    let p = world.player;
    let pool = world.ctx.alive_characters(p)
        .filter(|c| c.house_id != p.house_id && c.levy > 0);
    if pool.len() == 0 { return []; }
    let idx = world.ctx.rng(0, pool.len() - 1);
    [pool[idx]]
}

fn effect(world) {
    if world.choice_idx == 0 {
        world.ctx.transfer_gold(world.player, world.characters[0], 50);
    } else if world.choice_idx == 1 {
        world.ctx.transfer_gold(world.player, world.characters[0], 10);
    }
}

//! Authored event defs and runtime instances.
//!
//! ponytail: hard-coded in Rust for the first cut. Three events share the
//! roster; expand the slice + extend the `pick_attendee` match in
//! `presenting_event` to grow. RON-driven event authoring is one file in
//! `mods::load_event_definitions` away — the `EventDef` struct is the
//! serialisation target.

use bevy::prelude::Entity;

/// What a single choice does mechanically. The narrative observer and the
/// resolver both branch on this. Effects that need both directions of a
/// transaction (player pays / player receives) live here so the resolver
/// stays mechanical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceEffect {
    /// No mechanical effect — the choice exists for narrative consequence only.
    None,
    /// `from = player, to = attendee` — player pays `amount` gold.
    /// Spawns a `ReceivedGold { amount }` memory on the attendee toward the
    /// player, boosting the attendee's opinion of the player.
    GiveGold { amount: i64 },
    /// `from = attendee, to = player` — player receives `amount` gold.
    /// Spawns a `ReceivedGold { amount }` memory on the player toward the
    /// attendee, boosting the player's opinion of the attendee.
    ReceiveGold { amount: i64 },
}

/// One choice shown in the event popup.
pub struct ChoiceDef {
    /// Player-facing label rendered as the choice row.
    pub text: &'static str,
    pub effect: ChoiceEffect,
}

/// One authored event. Lives in `EVENT_DEFS` as a `&'static [ChoiceDef]`
/// alongside its other fields. `id` is the stable lookup key.
pub struct EventDef {
    /// Stable id (e.g. `"event:wayfaring_stranger"`). Used as the chronicle
    /// branch key and as a logging handle.
    pub id: &'static str,
    /// Popup title (one short line, e.g. "A wayfaring stranger at your gates").
    pub title: &'static str,
    /// Popup body. May include `{name}` placeholder, substituted at render
    /// time with the resolved attendee's display name (player if no attendee).
    pub narration: &'static str,
    /// Weighted draw — relative weights, not probabilities. The draw uses the
    /// sum of weights as the roll range.
    pub weight: u32,
    pub choices: &'static [ChoiceDef],
}

/// The running state of one in-flight event. Stored on `EventDeck::pending`;
/// the popup renders from it. `attendee` is `Some(e)` for events with a
/// named character, `None` for ambient events (none in the first cut).
pub struct EventInstance {
    /// Index into `EVENT_DEFS` — the chosen event's authored definition.
    pub def_index: usize,
    /// Resolved at presentation time so a choice can't drift between the
    /// narration and the effect (e.g. the envoy's house leader).
    pub attendee: Option<Entity>,
}

/// The author-controlled roster. Order is irrelevant — `weight` selects. Add
/// events here and grow the `pick_attendee` arms in `presenting_event`.
pub const EVENT_DEFS: &[EventDef] = &[
    EventDef {
        id: "event:wayfaring_stranger",
        title: "A wayfaring stranger at your gates",
        narration: "A weary traveller in plain clothes hails you from below the \
            battlements. They introduce themselves as {name}, and quietly ask \
            whether the court might spare a few coins for a hot meal and a bed.",
        weight: 3,
        choices: &[
            ChoiceDef {
                text: "Give 10 gold",
                effect: ChoiceEffect::GiveGold { amount: 10 },
            },
            ChoiceDef {
                text: "Send them away",
                effect: ChoiceEffect::None,
            },
        ],
    },
    EventDef {
        id: "event:envoy_house",
        title: "An envoy from a foreign house",
        narration: "An envoy in the colours of another great house bows low in \
            your hall. They present the greetings of {name}, their lord or lady, \
            and put forward a small request on their master's behalf.",
        weight: 2,
        choices: &[
            ChoiceDef {
                text: "Lend them 30 gold in good faith",
                effect: ChoiceEffect::GiveGold { amount: 30 },
            },
            ChoiceDef {
                text: "Accept their 25 gold tribute",
                effect: ChoiceEffect::ReceiveGold { amount: 25 },
            },
            ChoiceDef {
                text: "Turn them away",
                effect: ChoiceEffect::None,
            },
        ],
    },
    EventDef {
        id: "event:foreign_knight",
        title: "A foreign knight seeks service",
        narration: "An armoured figure kneels in your court. They give their \
            name as {name} — an old warrior whose sword arm is still sound. \
            They ask only for coin enough to settle the debts they leave behind \
            before swearing to your banner.",
        weight: 1,
        choices: &[
            ChoiceDef {
                text: "Welcome them — 50 gold",
                effect: ChoiceEffect::GiveGold { amount: 50 },
            },
            ChoiceDef {
                text: "Welcome them — 10 gold",
                effect: ChoiceEffect::GiveGold { amount: 10 },
            },
            ChoiceDef {
                text: "Send them away",
                effect: ChoiceEffect::None,
            },
        ],
    },
];

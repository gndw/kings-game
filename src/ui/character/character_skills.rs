//! Character skills: the six medieval-genuine stats (martial, prowess,
//! treasury, prudence, intrigue, faith) colour-tiered per cell.

use bevy::color::palettes::css;
use bevy::prelude::*;

const SKILL_GRAY: Color = Color::srgb(0.55, 0.55, 0.55);
const SKILL_WHITE: Color = Color::WHITE;
const SKILL_GREEN: Color = Color::Srgba(css::GREEN);

pub(super) fn render_skills_spans(skills: (i32, i32, i32, i32, i32, i32)) -> Vec<(String, Color)> {
    let pairs = [
        ("m:", skills.0),
        (" p:", skills.1),
        (" t:", skills.2),
        (" pr:", skills.3),
        (" i:", skills.4),
        (" f:", skills.5),
    ];
    let mut spans: Vec<(String, Color)> = vec![("skill: ".to_string(), Color::WHITE)];
    spans.extend(
        pairs
            .into_iter()
            .map(|(label, val)| (format!("{}{}", label, val), skill_color(val))),
    );
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

/// Skill colour tier: ≥15 green, 10..=14 white, else gray. Matches the
/// levy scheme in [`crate::ui::kingdom`] (red raised, yellow partial,
/// green max, gray idle) for visual consistency.
fn skill_color(value: i32) -> Color {
    if value >= 15 {
        SKILL_GREEN
    } else if value >= 10 {
        SKILL_WHITE
    } else {
        SKILL_GRAY
    }
}

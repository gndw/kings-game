//! Character header: name, house, gender, age, opinion, and the kingdom
//! link (when the character leads one).

use crate::ecs::character::CharacterGender;
use crate::helper::opinion_helper::get_opinion_color;
use bevy::prelude::*;

use super::super::TITLE;

#[allow(clippy::too_many_arguments)]
pub(super) fn render_detail_spans(
    name: &str,
    house: &str,
    gender: CharacterGender,
    char_age: u32,
    kingdom_name: Option<&str>,
    opinion: Option<i32>,
) -> Vec<(String, Color)> {
    let marker = match gender {
        CharacterGender::Male => "m",
        CharacterGender::Female => "f",
    };

    let mut spans: Vec<(String, Color)> = Vec::new();
    // Header line: name house [gender] (age) [+opinion]
    spans.push((format!("{} {}", name, house), Color::WHITE));
    spans.push((format!(" [{}] ({})", marker, char_age), Color::WHITE));
    if let Some(op) = opinion {
        spans.push((" [".to_string(), Color::WHITE));
        spans.push((format!("{:+}", op), get_opinion_color(op)));
        spans.push(("]".to_string(), Color::WHITE));
    }
    spans.push(("\n".to_string(), Color::WHITE));

    if let Some(kn) = kingdom_name {
        spans.push(("ruler of: ".to_string(), Color::WHITE));
        spans.push((format!("{}\n", kn), TITLE));
    }

    spans
}

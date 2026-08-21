//! Character stats: gold, gold yield, levy.

use bevy::color::palettes::css;
use bevy::prelude::*;

const LEVY_GREEN: Color = Color::Srgba(css::GREEN);
const LOSS_RED: Color = Color::Srgba(css::RED);
const GOLD_COLOR: Color = Color::Srgba(css::GOLD);

pub(super) fn render_stats_spans(gold: i64, gold_yield: i64, levy: u64) -> Vec<(String, Color)> {
    let mut spans: Vec<(String, Color)> = Vec::new();
    spans.push((format!("gold: {}\n", format_signed(gold)), Color::WHITE));
    let yield_color = if gold_yield >= 0 { GOLD_COLOR } else { LOSS_RED };
    spans.push((
        format!("gold/m: {}\n", format_signed(gold_yield)),
        yield_color,
    ));
    spans.push((format!("levy: {}\n", levy), LEVY_GREEN));
    spans
}

/// `+123` / `-45` with thousands separators and a sign so the panel reads
/// cleanly across the gold and yield rows.
fn format_signed(value: i64) -> String {
    if value >= 0 {
        format!("+{}", format_int(value as u64))
    } else {
        format!("-{}", format_int(value.unsigned_abs()))
    }
}

fn format_int(value: u64) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

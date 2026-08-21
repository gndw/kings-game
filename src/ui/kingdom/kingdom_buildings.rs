//! Kingdom building rows — one per building, color-coded by status
//! (gray inactive/building, red raised, yellow partial levy, white max
//! levy). Profit / max levy / upkeep show only when > 0.

use crate::ecs::building::{
    BuildingIsRaised, BuildingLevy, BuildingOf, BuildingStatus,
};
use crate::ecs::LandHasBuildings;
use crate::resources::buildings::BuildingDefs;
use bevy::color::palettes::css;
use bevy::prelude::*;

use super::super::TITLE;

const RAISED_RED: Color = Color::Srgba(css::RED);
const PARTIAL_YELLOW: Color = Color::Srgba(css::YELLOW);
const LEVY_GREEN: Color = Color::Srgba(css::GREEN);
const UPKEEP_GRAY: Color = Color::srgb(0.55, 0.55, 0.55);
const BUILDING_GRAY: Color = Color::srgba(0.55, 0.55, 0.55, 1.0);
const GOLD_COLOR: Color = Color::Srgba(css::GOLD);

pub(super) fn render_buildings_spans(world: &mut World, land_e: Entity) -> Vec<(String, Color)> {
    let buildings: Vec<Entity> = world
        .get::<LandHasBuildings>(land_e)
        .map(|l| l.iter().collect())
        .unwrap_or_default();
    if buildings.is_empty() {
        return Vec::new();
    }

    // Build QueryStates up-front (each takes &mut World momentarily); the
    // BuildingDefs resource is fetched inline with `.cloned()` so it never
    // holds a borrow across a query call.
    let mut of_q = world.query::<&BuildingOf>();
    let mut status_q = world.query::<&BuildingStatus>();
    let mut levy_q = world.query::<&BuildingLevy>();
    let mut raised_q = world.query::<&BuildingIsRaised>();

    struct Row {
        name: String,
        name_color: Color,
        // Spec: profit, max levy, upkeep — each shown when > 0.
        profit: Option<u32>,
        levy: Option<u32>,
        upkeep: Option<u32>,
    }
    let mut rows: Vec<Row> = Vec::new();
    for building_e in buildings {
        let Some(bof) = of_q.get(world, building_e).ok() else { continue };
        let Some(d) = world.resource::<BuildingDefs>().get(&bof.0).cloned() else {
            continue;
        };
        let status = status_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingStatus::Active);
        let is_raised = raised_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingIsRaised(false))
            .0;
        let current_levy = levy_q
            .get(world, building_e)
            .copied()
            .unwrap_or(BuildingLevy(0))
            .0;
        let max_levy = d.levy;

        let (name, name_color) = match status {
            BuildingStatus::Inactive | BuildingStatus::Building => {
                (d.name.clone(), BUILDING_GRAY)
            }
            BuildingStatus::Active => {
                if is_raised {
                    (d.name.clone(), RAISED_RED)
                } else if current_levy < max_levy {
                    (
                        format!("{} ({}/{})", d.name, current_levy, max_levy),
                        PARTIAL_YELLOW,
                    )
                } else {
                    (d.name.clone(), Color::WHITE)
                }
            }
        };
        rows.push(Row {
            name,
            name_color,
            profit: if d.gold_profit > 0 { Some(d.gold_profit) } else { None },
            levy: if d.levy > 0 { Some(d.levy) } else { None },
            upkeep: if d.gold_upkeep > 0 { Some(d.gold_upkeep) } else { None },
        });
    }
    if rows.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(String, Color)> = vec![("buildings:\n".to_string(), TITLE)];
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            spans.push(("\n".to_string(), Color::WHITE));
        }
        spans.push((format!("{} ", row.name), row.name_color));
        if let Some(p) = row.profit {
            spans.push((format!("+{}g ", p), GOLD_COLOR));
        }
        if let Some(l) = row.levy {
            spans.push((format!("{} ", l), LEVY_GREEN));
        }
        if let Some(u) = row.upkeep {
            spans.push((format!("-{}g", u), UPKEEP_GRAY));
        }
    }
    spans.push(("\n".to_string(), Color::WHITE));
    spans
}

//! Which hooks a tick fires, and calling them.
//!
//! The whole list is `on_startup`, `on_day` and `on_month`. Add one here and in
//! the README's script section; a mod defines any, all, or none of them.

use super::{ModScript, ScriptCtx};
use crate::date::Date;
use rhai::{Engine, Scope};

/// Fires once, before the first tick, so a script can publish whatever the
/// opening screen should already show rather than a day's worth of zeroes.
pub(super) const STARTUP: [&str; 1] = ["on_startup"];

/// The hooks due on `date`, in the order they fire. `on_month` only ever fires
/// on day 1, so a script that defines it needs no date check of its own.
pub(super) fn due(date: &Date) -> Vec<&'static str> {
    let mut hooks = vec!["on_day"];
    if date.is_month_start() {
        hooks.push("on_month");
    }
    hooks
}

/// Call each of `hooks` on every mod, in load order. Returns whoever threw, as
/// `(index into mods, mod name, message)` — the caller decides what happens to
/// them. A mod that throws is skipped for the rest of *this* tick too: its
/// later hooks would run against the half-applied state of the one that blew up.
pub(super) fn call(
    engine: &Engine,
    mods: &[ModScript],
    sctx: &ScriptCtx,
    hooks: &[&str],
) -> Vec<(usize, String, String)> {
    let mut broken = Vec::new();
    for (i, m) in mods.iter().enumerate() {
        for hook in hooks {
            // Checking the AST beats calling and swallowing a not-found
            // error: a mod defines either hook, both, or neither.
            if !m
                .ast
                .iter_functions()
                .any(|f| f.name == *hook && f.params.len() == 1)
            {
                continue;
            }
            let mut scope = Scope::new();
            let call = engine.call_fn::<()>(&mut scope, &m.ast, hook, (sctx.clone(),));
            if let Err(e) = call {
                broken.push((i, m.name.clone(), e.to_string()));
                break;
            }
        }
    }
    broken
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;

    #[test]
    fn hooks_fire_daily_and_monthly_across_separate_files() {
        // One hook per file, the way the base game ships them. Each file is a
        // separate AST, so this also proves two scripts in one folder both run.
        let dir = mods_dir(
            "hooks",
            &[
                ("base/world.ron", WORLD),
                (
                    "base/on_startup.rhai",
                    r#"fn on_startup(ctx) { ctx.add_chronicle("startup"); }"#,
                ),
                (
                    "base/on_day.rhai",
                    r#"fn on_day(ctx) { ctx.add_chronicle("day " + ctx.day); }"#,
                ),
                (
                    "base/on_month.rhai",
                    r#"fn on_month(ctx) { ctx.add_chronicle("month " + ctx.month); }"#,
                ),
            ],
        );
        let (lines, _) = play(&dir, 31);
        // 31 days of `on_day`, plus `on_month` on the one day-1 in that span,
        // plus the single `on_startup` that precedes the lot.
        assert_eq!(lines.len(), 33);
        assert_eq!(lines[0], "startup", "startup runs before the first tick");
        assert_eq!(lines.iter().filter(|l| *l == "startup").count(), 1);
        assert_eq!(lines[1], "day 2");
        assert!(lines.contains(&"month 2".to_string()));
        assert_eq!(lines.iter().filter(|l| l.starts_with("month")).count(), 1);
    }
}

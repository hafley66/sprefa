//! R7 diag stage routing: presentation-time filtering of the `diag` sink by
//! stage. The db keeps EVERY diagnostic row; a stage selects the subset a given
//! surface shows — a live daemon tick, the commit `--check`, an agent turn, or
//! an agent session start.
//!
//! Routing is colocated in the `.dl` program: a rail heads
//! `diag_stage(code, stage)` beside its `diag(...)` rule (no central registry).
//! A code with NO `diag_stage` row falls to the severity default — an error
//! routes to every stage, a warning to `commit` only — so the audit-tier
//! warnings stop spamming live ticks and agent turns until a rail opts them in.
//! That single default preserves the historical `dl --check` surface (at the
//! `commit` stage every severity is still routed) while fixing the annoyance.
//!
//! Pure functions, no storage: the filter runs at render time in
//! `--check`/`--hook`; querying `? diag(...)` stays complete.

use std::collections::{HashMap, HashSet};

use crate::engine::DiagRow;

/// The four routing stages. `live` = every daemon-tick surface; `commit` =
/// `--check` / pre-commit; `agent-turn` / `agent-session` = `--hook` events.
pub const STAGES: [&str; 4] = ["live", "commit", "agent-turn", "agent-session"];

/// The default stage for `--check` (and any unrouted surface): the pre-commit
/// gate, whose output is unchanged from before R7.
pub const DEFAULT_STAGE: &str = "commit";

/// Is `name` one of the four routing stages (for the `--stage` flag)?
pub fn is_stage(name: &str) -> bool {
    STAGES.contains(&name)
}

/// The stages a code with no `diag_stage` row routes to, by severity: an
/// `error` reaches every stage; a `warning` (or anything not `error`) reaches
/// `commit` only.
pub fn default_stages(severity: &str) -> &'static [&'static str] {
    if severity == "error" {
        &STAGES
    } else {
        &["commit"]
    }
}

/// Session-boundary events (Claude Code / codex `SessionStart` / `SessionEnd`)
/// carry the `agent-session` summary; every other hook event is an
/// `agent-turn`.
pub fn is_session_event(kind: &str) -> bool {
    matches!(kind, "SessionStart" | "SessionEnd")
}

/// Build the `code -> stages` routing map from `diag_stage` rows (each a
/// `[code, stage]` pair). A code may carry several rows (one per stage it opts
/// into); duplicates collapse.
pub fn routes_from_rows(rows: &[Vec<String>]) -> HashMap<String, Vec<String>> {
    let mut routes: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (Some(code), Some(stage)) = (row.first(), row.get(1)) else {
            continue;
        };
        let entry = routes.entry(code.clone()).or_default();
        if !entry.iter().any(|s| s == stage) {
            entry.push(stage.clone());
        }
    }
    routes
}

/// Is a diagnostic `(code, severity)` routed to `stage`? An explicit
/// `diag_stage` row for the code overrides the severity default (colocated
/// wins).
pub fn routed_to(
    code: &str,
    severity: &str,
    stage: &str,
    routes: &HashMap<String, Vec<String>>,
) -> bool {
    match routes.get(code) {
        Some(stages) => stages.iter().any(|s| s == stage),
        None => default_stages(severity).contains(&stage),
    }
}

/// Keep the diags routed to `stage`.
pub fn stage_filter(
    diags: Vec<DiagRow>,
    stage: &str,
    routes: &HashMap<String, Vec<String>>,
) -> Vec<DiagRow> {
    diags
        .into_iter()
        .filter(|d| routed_to(&d.code, &d.severity, stage, routes))
        .collect()
}

/// `agent-turn` gate: keep only diags whose path is in the latest-turn touch
/// set (`agent_edit` / `agent_touch`), so an agent hears about what it just
/// touched, never the whole audit list.
pub fn touched_only(diags: Vec<DiagRow>, touched: &HashSet<String>) -> Vec<DiagRow> {
    diags
        .into_iter()
        .filter(|d| touched.contains(&d.path))
        .collect()
}

/// `agent-session` summary: per-code counts (descending, code-tie-broken) plus
/// the first `top_n` diags as a sample.
pub fn session_summary(diags: &[DiagRow], top_n: usize) -> (Vec<(String, usize)>, Vec<&DiagRow>) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for d in diags {
        *counts.entry(d.code.as_str()).or_default() += 1;
    }
    let mut counts: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(c, n)| (c.to_string(), n))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let sample = diags.iter().take(top_n).collect();
    (counts, sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(path: &str, severity: &str, code: &str, msg: &str) -> DiagRow {
        DiagRow {
            path: path.into(),
            line: 1,
            col: 0,
            end_line: 1,
            end_col: 0,
            severity: severity.into(),
            code: code.into(),
            msg: msg.into(),
            hint: None,
        }
    }

    #[test]
    fn error_routes_to_every_stage_by_default() {
        let routes = HashMap::new();
        for stage in STAGES {
            assert!(
                routed_to("any", "error", stage, &routes),
                "error must reach {stage}"
            );
        }
    }

    #[test]
    fn unrouted_warning_only_at_commit() {
        let routes = HashMap::new();
        assert!(routed_to("todo", "warning", "commit", &routes));
        assert!(!routed_to("todo", "warning", "live", &routes));
        assert!(!routed_to("todo", "warning", "agent-turn", &routes));
        assert!(!routed_to("todo", "warning", "agent-session", &routes));
    }

    #[test]
    fn colocated_diag_stage_rows_override_the_default() {
        // A warning explicitly routed to agent-turn is NO LONGER at commit
        // (the row list is exhaustive, not additive to the default).
        let rows = vec![
            vec!["storm-syntax".to_string(), "agent-turn".to_string()],
            vec!["storm-syntax".to_string(), "agent-session".to_string()],
        ];
        let routes = routes_from_rows(&rows);
        assert!(routed_to("storm-syntax", "warning", "agent-turn", &routes));
        assert!(routed_to(
            "storm-syntax",
            "warning",
            "agent-session",
            &routes
        ));
        assert!(!routed_to("storm-syntax", "warning", "commit", &routes));
        assert!(!routed_to("storm-syntax", "warning", "live", &routes));
    }

    #[test]
    fn colocated_override_pins_an_error_off_a_stage() {
        // An explicit row set overrides even the error default: an error routed
        // only to commit drops off the agent surfaces.
        let rows = vec![vec!["build-break".to_string(), "commit".to_string()]];
        let routes = routes_from_rows(&rows);
        assert!(routed_to("build-break", "error", "commit", &routes));
        assert!(!routed_to("build-break", "error", "agent-turn", &routes));
    }

    #[test]
    fn stage_filter_keeps_the_routed_subset() {
        let routes = HashMap::new();
        let diags = vec![
            diag("a.rs", "error", "e1", "boom"),
            diag("b.rs", "warning", "w1", "meh"),
        ];
        // commit: both (error everywhere, warning at commit).
        assert_eq!(stage_filter(diags.clone(), "commit", &routes).len(), 2);
        // live: only the error.
        let live = stage_filter(diags, "live", &routes);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].code, "e1");
    }

    #[test]
    fn touched_only_intersects_the_turn_edit_set() {
        let diags = vec![
            diag("touched.rs", "warning", "w1", "a"),
            diag("elsewhere.rs", "warning", "w1", "b"),
        ];
        let touched: HashSet<String> = ["touched.rs".to_string()].into_iter().collect();
        let kept = touched_only(diags, &touched);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].path, "touched.rs");
    }

    #[test]
    fn session_summary_counts_per_code_descending() {
        let diags = vec![
            diag("a.rs", "warning", "todo", "x"),
            diag("b.rs", "warning", "todo", "y"),
            diag("c.rs", "warning", "fixme", "z"),
        ];
        let (counts, sample) = session_summary(&diags, 2);
        assert_eq!(
            counts,
            vec![("todo".to_string(), 2), ("fixme".to_string(), 1)]
        );
        assert_eq!(sample.len(), 2);
    }

    #[test]
    fn is_stage_and_session_event_names() {
        assert!(is_stage("agent-session"));
        assert!(!is_stage("nope"));
        assert!(is_session_event("SessionStart"));
        assert!(!is_session_event("PostToolUse"));
    }
}

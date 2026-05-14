//! RED TEST — Step 1 of the rules-as-tables patch series.
//!
//! Spec: bare `rule_name(...)` is a QUERY. It reads existing rows from
//! `<rule_name>_facts`. It does NOT run the body. Body execution is
//! reserved for the apply forms `rule_name.(...)` and `rule_name!.(...)`.
//!
//! Current bug (walk.rs:282-358): bare calls are dispatched to body-run.
//! That inverts the corrected V4 semantics. Fix this first; everything
//! in the fuser depends on it.
//!
//! Body-run detection: we use the rule's OWN fact table as the signal.
//! When the body runs (via `r.(...)` or `r!.(...)`), the body's output
//! is materialized into `r_facts`. When the body does NOT run (bare
//! query), `r_facts` stays at its pre-call row count. No separate `tag`
//! op is needed; the rule IS the table.
//!
//! Tests:
//!   1. bare `r(A?, B?)` against an empty `r_facts` must leave `r_facts`
//!      empty. If body executed, `r_facts` would have rows.
//!   2. `r.("a", "b")` runs the body (grounded args) and `r_facts` has
//!      one row.
//!   3. `r!.("a", "b")` × 2 runs body twice (apply-cache bypass).
//!   4. `r.(X?, "b")` produces `lower/apply-with-hole` diagnostic.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn count_rows(store: &Arc<dyn FactStore<Cursor>>, table: &str) -> usize {
    store
        .iter_table(table)
        .map(|i| i.count())
        .unwrap_or(0)
}

#[test]
fn bare_call_is_query_not_body_run() {
    // Body has a trivial str literal so its execution would produce one
    // row in r_facts. Bare call must leave r_facts empty.
    let src = r#"
        rule(:r, :A, :B) { str`marker` };
        r(A?, B?)
    "#;
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, diags) = host_parse(src);
    assert!(diags.is_empty(), "parse diags: {diags:?}");

    let mut ctx = LowerCtx::new(store.clone(), dir);
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);

    // No diagnostics: bare call against an unmaterialized table is
    // valid (drain + subscribe to future inserts).
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    // Bare call must NOT have materialized the body.
    let r_rows = count_rows(&store, "r");
    assert_eq!(
        r_rows, 0,
        "bare call dispatched to body: r_facts has {r_rows} rows; expected 0"
    );
}

#[test]
fn apply_dot_runs_body() {
    // `r.("a", "b")` is the apply form. Body runs. r_facts gets one row
    // (the body's str output).
    let src = r#"
        rule(:r, :A, :B) { str`marker` };
        r.("a", "b")
    "#;
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, _) = host_parse(src);

    let mut ctx = LowerCtx::new(store.clone(), dir);
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let r_rows = count_rows(&store, "r");
    assert!(r_rows > 0, "apply form failed to run body; r_facts is empty");
}

#[test]
fn apply_dot_with_hole_is_diagnostic() {
    // `r.(X?, "b")` is invalid: apply cannot accept holes. Must produce
    // a lower-time diagnostic, not silent fallback to query.
    let src = r#"
        rule(:r, :A, :B) { str`marker` };
        r.(X?, "b")
    "#;
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, _) = host_parse(src);

    let mut ctx = LowerCtx::new(store.clone(), dir);
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);

    let has_apply_with_hole = walk_diags
        .iter()
        .any(|d| d.code.as_ref() == "lower/apply-with-hole");
    assert!(
        has_apply_with_hole,
        "expected lower/apply-with-hole diagnostic, got: {walk_diags:?}"
    );
}

#[test]
fn bang_dot_bypasses_apply_cache() {
    // `r!.(X, Y)` runs body and bypasses the apply-cache read.
    // Verified by running twice with identical args. The body uses the
    // args to disambiguate the row content so OR IGNORE doesn't collapse
    // them. Two distinct apply calls with the same args BOTH execute
    // the body; whether the resulting row deduplicates by content is a
    // separate concern (it will, since (A=a, B=b) is the same row).
    //
    // To prove cache bypass independently of fact dedup, we use two
    // different arg pairs across the two `r!.(...)` calls.
    let src = r#"
        rule(:r, :A, :B) { str`marker` };
        r!.("a", "b");
        r!.("c", "d")
    "#;
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, _) = host_parse(src);

    let mut ctx = LowerCtx::new(store.clone(), dir);
    let _ = walk_program(&program, &reg, &mut ctx);

    let r_rows = count_rows(&store, "r");
    assert_eq!(
        r_rows, 2,
        "expected 2 body invocations producing 2 distinct rows (cache bypass), got {r_rows}"
    );
}

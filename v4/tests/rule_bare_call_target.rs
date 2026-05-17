//! V4 locked rule semantics (no polymorphism, no apply form):
//!
//!   r(X, Y)    → CALL: run the body / write. Args must be grounded.
//!   r!(X, Y)   → CALL bypassing the invoke-cache.
//!   r?(X?, Y?) → QUERY <r>_facts. Holes/kwargs project; body never runs.
//!   r.(...)    → REMOVED. Hard error `lower/rule-dotted-apply`.
//!
//! Body-run detection: a rule IS its fact table. When the body runs
//! (a grounded call), output is materialized into `r_facts`. A query, a
//! call with a hole (error), or a dotted apply (error) leaves it empty.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn count_rows(store: &Arc<dyn FactStore<Cursor>>, table: &str) -> usize {
    store.iter_table(table).map(|i| i.count()).unwrap_or(0)
}

fn walk(src: &str) -> (Arc<dyn FactStore<Cursor>>, Vec<effect_runtime::v2::Diag>) {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let (program, pdiags) = host_parse(src);
    assert!(pdiags.is_empty(), "parse diags: {pdiags:?}");
    let mut ctx = LowerCtx::new(store.clone(), std::env::temp_dir());
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    (store, walk_diags)
}

#[test]
fn bare_call_grounded_runs_body() {
    // `r("a", "b")` is a CALL. Body runs, r_facts gets the str output.
    let (store, diags) = walk(r#"
        rule(:r, :A, :B) { str`marker` };
        r("a", "b")
    "#);
    assert!(diags.is_empty(), "walk diags: {diags:?}");
    assert!(
        count_rows(&store, "r") > 0,
        "bare grounded call did not run body; r_facts empty"
    );
}

#[test]
fn bare_call_with_hole_is_error() {
    // A call cannot accept holes — use the query form `r?(...)`.
    let (store, diags) = walk(r#"
        rule(:r, :A, :B) { str`marker` };
        r(A?, B?)
    "#);
    assert!(
        diags.iter().any(|d| d.code.as_ref() == "lower/call-with-hole"),
        "expected lower/call-with-hole, got: {diags:?}"
    );
    assert_eq!(count_rows(&store, "r"), 0, "errored call must not write");
}

#[test]
fn query_form_reads_table_never_runs_body() {
    // `r?(A?, B?)` queries r_facts. Body never runs; table stays empty.
    let (store, diags) = walk(r#"
        rule(:r, :A, :B) { str`marker` };
        r?(A?, B?)
    "#);
    assert!(diags.is_empty(), "walk diags: {diags:?}");
    assert_eq!(
        count_rows(&store, "r"),
        0,
        "query must not run body; r_facts has rows"
    );
}

#[test]
fn dotted_apply_is_removed() {
    // `r.(...)` is gone for good. Hard error, no body run.
    let (store, diags) = walk(r#"
        rule(:r, :A, :B) { str`marker` };
        r.("a", "b")
    "#);
    assert!(
        diags
            .iter()
            .any(|d| d.code.as_ref() == "lower/rule-dotted-apply"),
        "expected lower/rule-dotted-apply, got: {diags:?}"
    );
    assert_eq!(count_rows(&store, "r"), 0, "removed form must not write");
}

#[test]
fn bang_call_bypasses_cache_and_runs_body() {
    // `r!(...)` is a force-call: runs body, bypasses the invoke-cache.
    // Two distinct grounded calls produce two distinct rows.
    let (store, diags) = walk(r#"
        rule(:r, :A, :B) { str`marker` };
        r!("a", "b");
        r!("c", "d")
    "#);
    assert!(diags.is_empty(), "walk diags: {diags:?}");
    assert_eq!(
        count_rows(&store, "r"),
        2,
        "expected 2 forced body invocations, got {}",
        count_rows(&store, "r")
    );
}

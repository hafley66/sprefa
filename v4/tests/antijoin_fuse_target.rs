//! RED TEST — `not(rule?(args))` antijoin surface (option A).
//!
//! Spec: a rule body containing `not(<rule>?(args))` lifts to a
//! `NOT EXISTS (SELECT 1 FROM <rule>_facts AS ax<n> WHERE <preds>)`
//! filter inside the full-SQL kind. The antijoin produces no
//! captures; binders correlate inner cols to outer captures or
//! literals.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn lower(src: &str) -> (Vec<v4::compile::FusedRule>, Vec<effect_runtime::v2::Diag>) {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");
    let mut ctx = LowerCtx::new(store, dir);
    let (rules, walk_diags) = walk_program(&program, &reg, &mut ctx);
    (rules, walk_diags)
}

#[test]
fn antijoin_emits_not_exists_subquery() {
    let src = r#"
        rule(:hits,    :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:masked,  :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`SUPPRESS(${LO?})` };
        rule(:visible, :FS, :LO) {
            hits?(FS?, LO?) > not(masked?(FS, LO))
        }
    "#;
    let (rules, diags) = lower(src);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let visible = rules.iter().find(|r| r.name() == "visible").unwrap();
    assert!(
        visible.is_full_sql(),
        "visible must be full-SQL, got {:?}",
        visible.kind()
    );

    let sql = visible
        .fused_sql()
        .expect("full-SQL kind must expose its SQL");
    assert!(
        sql.contains("NOT EXISTS"),
        "expected NOT EXISTS subquery, got: {sql}"
    );
    assert!(
        sql.contains("masked_facts"),
        "subquery must reference masked_facts, got: {sql}"
    );
    // The antijoin's inner subalias should correlate against r0 (the
    // first outer rule-query). Bare-name check rather than full equality
    // string so future column renames don't break the assert.
    assert!(
        sql.contains("ax1.") || sql.contains("ax0."),
        "expected `axN.` subalias from antijoin emission, got: {sql}"
    );
}

#[test]
fn antijoin_rejects_projection_binder() {
    let src = r#"
        rule(:hits,   :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:masked, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`SUPPRESS(${LO?})` };
        rule(:bad,    :FS, :LO) {
            hits?(FS?, LO?) > not(masked?(FS, LO?))
        }
    "#;
    let (_rules, diags) = lower(src);
    assert!(
        diags.iter().any(|d| d.code.as_ref() == "lower/antijoin-bind"),
        "expected lower/antijoin-bind diag, got: {diags:?}"
    );
}

#[test]
fn antijoin_rejects_non_query_inner() {
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:masked, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`SUPPRESS(${LO?})` };
        rule(:bad, :FS, :LO) {
            hits?(FS?, LO?) > not(masked(FS, LO))
        }
    "#;
    let (_rules, diags) = lower(src);
    assert!(
        diags
            .iter()
            .any(|d| d.code.as_ref() == "lower/antijoin-shape"),
        "expected lower/antijoin-shape diag, got: {diags:?}"
    );
}

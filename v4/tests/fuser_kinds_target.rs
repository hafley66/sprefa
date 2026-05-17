//! RED TEST — Step 5 of the rules-as-tables patch series.
//!
//! Spec: the fuser is a peephole pass at the top of `walk_pipe` that
//! consumes the TermFlowGraph (step 3) and the per-op `Liftable`
//! classifications (step 4) and picks ONE of two kinds per rule body:
//!
//!   full-SQL       — when every body op is Pure or RuleQuery.
//!                    Emits ONE `INSERT OR IGNORE INTO <rule>_facts
//!                    SELECT ...` plus a support_edges insert in the
//!                    same transaction. Zero cursors cross Rust.
//!
//!   streamed-Rust  — when any body op is Stream.
//!                    Opens ONE prepared INSERT statement; Stream ops
//!                    run per row in Rust and write directly. No
//!                    Vec<Arc<Cursor>> between ops.
//!
//! Mixed Stream-then-RuleQuery is REJECTED at lower time with
//! `lower/rule-stream-after-relation`. User splits into two rules.
//!
//! The fuser also applies the five pre-computations from the
//! TermFlowGraph:
//!   P1 Dead-capture elimination
//!   P2 Predicate pushdown
//!   P3 Literal-pin folding (pre-intern literals at compile time)
//!   P4 Type-driven storage (id columns, not text)
//!   P5 Join-order selection

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
    // After step 5, walk_program returns FusedRule rather than raw Pipe,
    // so the test can introspect the chosen kind.
    (rules, walk_diags)
}

#[test]
fn full_sql_kind_when_body_is_pure_and_rule_query() {
    // hit_pairs body is two rule queries plus a where predicate. No
    // Stream ops. Must pick full-SQL.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits?(FS?, LO?) > hits?(FS?, OTHER_LO?) > where`${LO} != ${OTHER_LO}`
        }
    "#;
    let (rules, diags) = lower(src);
    assert!(diags.is_empty(), "diags: {diags:?}");

    let pairs = rules.iter().find(|r| r.name() == "hit_pairs").unwrap();
    assert!(
        pairs.is_full_sql(),
        "hit_pairs must be full-SQL kind, got {:?}",
        pairs.kind()
    );

    let sql = pairs.fused_sql().expect("full-SQL kind must expose its SQL");
    assert!(
        sql.contains("INSERT OR IGNORE INTO hit_pairs_facts"),
        "must INSERT into hit_pairs_facts, got: {sql}"
    );
    assert!(
        sql.contains("hits_facts") && sql.matches("hits_facts").count() >= 2,
        "must self-join hits_facts twice, got: {sql}"
    );
    assert!(
        sql.contains("support_edges"),
        "must populate support_edges in same transaction, got: {sql}"
    );
}

#[test]
fn streamed_rust_kind_when_body_has_stream_op() {
    // hits body has fs + ast (both Stream). Must pick streamed-Rust.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` }
    "#;
    let (rules, diags) = lower(src);
    assert!(diags.is_empty(), "diags: {diags:?}");

    let hits = rules.iter().find(|r| r.name() == "hits").unwrap();
    assert!(
        hits.is_streamed_rust(),
        "hits must be streamed-Rust kind, got {:?}",
        hits.kind()
    );

    // The streamed-Rust kind exposes its prepared-insert SQL so we
    // can confirm it's built once per rule, not per row.
    let prepared = hits.prepared_insert_sql().expect("streamed kind exposes prepared SQL");
    assert!(
        prepared.starts_with("INSERT OR IGNORE INTO hits_facts"),
        "prepared insert must target hits_facts, got: {prepared}"
    );
    assert!(
        prepared.contains("?,?"),
        "must use bind params, got: {prepared}"
    );
}

#[test]
fn mixed_stream_then_rule_query_is_diagnostic() {
    // Stream (fs) followed by a RuleQuery (other_rule(X?)) in the same
    // body is rejected. User must split.
    let src = r#"
        rule(:other, :FS) { fs(glob`**/*.c`) };
        rule(:bad, :FS, :X) {
            fs(glob`**/*.c`) > other(FS?)
        }
    "#;
    let (_rules, diags) = lower(src);

    let has_diag = diags
        .iter()
        .any(|d| d.code.as_ref() == "lower/rule-stream-after-relation");
    assert!(
        has_diag,
        "expected lower/rule-stream-after-relation, got: {diags:?}"
    );
}

#[test]
fn p1_dead_capture_elimination_drops_column() {
    // hits's body binds HI but the rule declares only FS, LO. HI must
    // be dropped from the INSERT projection.
    let src = r#"
        rule(:hits, :FS, :LO) {
            fs(glob`**/*.c`) > ast(:c)`printk(${LO?}, ${HI?})`
        }
    "#;
    let (rules, _) = lower(src);
    let hits = rules.iter().find(|r| r.name() == "hits").unwrap();
    let prepared = hits.prepared_insert_sql().unwrap();

    assert!(
        !prepared.contains("HI"),
        "HI is dead; must not appear in INSERT, got: {prepared}"
    );
}

#[test]
fn p2_predicate_pushdown_into_join_on_clause() {
    // hit_pairs's where predicate references only join-input columns.
    // P2 moves it from WHERE to ON.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits?(FS?, LO?) > hits?(FS?, OTHER_LO?) > where`${LO} != ${OTHER_LO}`
        }
    "#;
    let (rules, _) = lower(src);
    let pairs = rules.iter().find(|r| r.name() == "hit_pairs").unwrap();
    let sql = pairs.fused_sql().unwrap();

    // Predicate must fuse into JOIN ... ON ... AND a.LO <> b.LO.
    // Naive lowering would put it in a trailing WHERE.
    let on_section = sql
        .split("WHERE")
        .next()
        .unwrap_or("");
    assert!(
        on_section.contains("<>") || on_section.contains("!="),
        "predicate must be pushed into JOIN ON, got: {sql}"
    );
}

#[test]
fn p3_literal_pin_pre_interns_at_compile_time() {
    // A rule query with a literal arg must compile to an integer FK
    // comparison, not a TEXT comparison.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:q, :LO) { hits?("kernel/printk.c", LO?) }
    "#;
    let (rules, _) = lower(src);
    let q = rules.iter().find(|r| r.name() == "q").unwrap();
    let sql = q.fused_sql().unwrap();

    // Should compare against the pre-interned StringId, not the literal.
    // We can verify by: no quoted "kernel/printk.c" in the fused SQL,
    // and a numeric literal in the FS comparison.
    assert!(
        !sql.contains("'kernel/printk.c'"),
        "literal must be pre-interned; raw string should NOT appear in SQL: {sql}"
    );
}

#[test]
fn p4_id_typed_columns_in_facts_schema() {
    // hits_facts schema must use INTEGER REFERENCES, not TEXT.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` }
    "#;
    let (rules, _) = lower(src);
    let hits = rules.iter().find(|r| r.name() == "hits").unwrap();
    let create = hits.create_table_sql().expect("rule emits CREATE TABLE schema");

    assert!(
        create.contains("INTEGER") && !create.contains(" TEXT"),
        "fact table columns must be id-typed (INTEGER), no TEXT: {create}"
    );
    assert!(
        create.contains("_id BLOB PRIMARY KEY"),
        "_id must be BLOB primary key (blake3 raw bytes): {create}"
    );
    assert!(
        create.contains("WITHOUT ROWID"),
        "fact table should be WITHOUT ROWID for compact storage: {create}"
    );
}

#[test]
fn where_backtick_dsl_body_is_pushed_into_join_on_clause() {
    // Canonical surface for the where predicate is a backtick DSL body.
    // The fuser must read the predicate text from op.dsl, not op.args,
    // and produce the same JOIN ON predicate-pushdown shape as the
    // paren form.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits?(FS?, LO?) > hits?(FS?, OTHER_LO?) > where`${LO} != ${OTHER_LO}`
        }
    "#;
    let (rules, diags) = lower(src);
    assert!(
        diags.iter().all(|d| d.severity != effect_runtime::v2::Severity::Error),
        "backtick where must lower without errors, got: {diags:?}"
    );
    let pairs = rules.iter().find(|r| r.name() == "hit_pairs").unwrap();
    let sql = pairs.fused_sql().unwrap();
    let on_section = sql.split("WHERE").next().unwrap_or("");
    assert!(
        on_section.contains("<>") || on_section.contains("!="),
        "predicate must be pushed into JOIN ON, got: {sql}"
    );
}

#[test]
fn support_edges_populated_in_same_transaction() {
    // The fused full-SQL kind issues two INSERT...SELECT statements
    // in one transaction: one for facts, one for support edges.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits?(FS?, LO?) > hits?(FS?, OTHER_LO?)
        }
    "#;
    let (rules, _) = lower(src);
    let pairs = rules.iter().find(|r| r.name() == "hit_pairs").unwrap();
    let stmts = pairs.fused_statements().unwrap();

    assert!(
        stmts.iter().any(|s| s.contains("INSERT") && s.contains("support_edges")),
        "must include INSERT into support_edges: {stmts:?}"
    );
    assert_eq!(
        pairs.transaction_kind(),
        v4::compile::TransactionKind::Single,
        "facts and support_edges insert must share a single transaction"
    );
}

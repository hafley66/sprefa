//! Mid-stream `where` predicate evaluator — Stream → where → ...
//!
//! Pins the runtime behaviour of `where\`...\`` when the rule body
//! is streamed-Rust (any Stream op present). Today the fuser pushes
//! the predicate into JOIN ON only when the body is fully liftable
//! (Pure + RuleQuery). Anything else fell back to a VoidComponent —
//! the predicate silently no-op'd. These tests pin the new
//! `PredicateComponent` path.

use std::sync::Arc;

use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend, Severity,
};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

/// Compile + run `src` against fixture `root`. Returns the fact store
/// so callers can inspect each rule's emitted rows. Distinct from
/// `fs_pure_smoke::run_in` only in that walk diags are returned rather
/// than asserted empty (the parse-error test needs the diags).
fn run_in(
    root: &std::path::Path,
    src: &str,
) -> (
    Arc<dyn FactStore<Cursor>>,
    Vec<effect_runtime::v2::Diag>,
) {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), root.to_path_buf());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        let inst = fused.into_pipe().into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
    (store, walk_diags)
}

fn write_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    // Two .c files; each contains predictable function definitions
    // whose names the `re` named capture surfaces as cursor terms.
    // The `where` predicate filters on those term values.
    std::fs::write(
        tmp.path().join("a.c"),
        b"int main(void) { return 0; }\nint helper(int x) { return x; }\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.c"),
        b"int worker(int n) { return n; }\nint main(void) { return 0; }\n",
    )
    .unwrap();
    tmp
}

fn names_in(store: &Arc<dyn FactStore<Cursor>>, rel: &str) -> Vec<String> {
    let mut out: Vec<String> = store
        .rows_of(rel)
        .iter()
        .filter_map(|c| c.get("NAME").map(|s| s.to_string()))
        .collect();
    out.sort();
    out
}

// ── 1. Stream → where filters rows ─────────────────────────────────

#[test]
fn where_after_stream_op_filters_rows() {
    let tmp = write_fixture();
    let src = r#"
        rule(:fns, NAME?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${NAME} != 'main'`
        };
    "#;
    let (store, diags) = run_in(tmp.path(), src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "diags: {diags:?}"
    );
    let names = names_in(&store, "fns");
    assert!(!names.contains(&"main".to_string()), "names: {names:?}");
    assert!(names.contains(&"helper".to_string()), "names: {names:?}");
    assert!(names.contains(&"worker".to_string()), "names: {names:?}");
}

// ── 2. OR / AND combine into expected sets ─────────────────────────

#[test]
fn where_or_and_combine() {
    let tmp = write_fixture();
    let src = r#"
        rule(:fns, NAME?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${NAME} = 'helper' OR ${NAME} = 'worker'`
        };
    "#;
    let (store, diags) = run_in(tmp.path(), src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "diags: {diags:?}"
    );
    let names = names_in(&store, "fns");
    // Expect helper + worker only; main excluded.
    assert!(!names.contains(&"main".to_string()), "names: {names:?}");
    assert!(names.contains(&"helper".to_string()), "names: {names:?}");
    assert!(names.contains(&"worker".to_string()), "names: {names:?}");
}

// ── 3. IS NULL / IS NOT NULL filter rows by term presence ──────────

#[test]
fn where_is_null_and_not_null() {
    let tmp = write_fixture();

    // First regex binds NAME for every match. Second predicate IS NOT
    // NULL keeps every row (NAME is always set after `re`).
    let src_keep_all = r#"
        rule(:keep, NAME?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${NAME} IS NOT NULL`
        };
    "#;
    let (store, diags) = run_in(tmp.path(), src_keep_all);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "diags: {diags:?}"
    );
    assert!(
        store.rows_of("keep").len() >= 3,
        "expected helper+worker+two mains, got {}",
        store.rows_of("keep").len()
    );

    // IS NULL on the same captured name drops every row — NAME is
    // always bound by the regex.
    let tmp2 = write_fixture();
    let src_drop_all = r#"
        rule(:drop, NAME?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${NAME} IS NULL`
        };
    "#;
    let (store2, diags2) = run_in(tmp2.path(), src_drop_all);
    assert!(
        diags2.iter().all(|d| d.severity != Severity::Error),
        "diags: {diags2:?}"
    );
    assert_eq!(
        store2.rows_of("drop").len(),
        0,
        "IS NULL on always-bound term should drop every row"
    );
}

// ── 4. Numeric compare on the LO term ──────────────────────────────

#[test]
fn where_numeric_compare() {
    // The first int-decl in `a.c` is `main` at byte 0; helper follows
    // after the newline at offset 29. LO > 10 must drop main and keep
    // helper (and similarly drop one of b.c's matches).
    let tmp = write_fixture();
    let src = r#"
        rule(:later_fns, NAME?, LO?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${LO} > 10`
        };
    "#;
    let (store, diags) = run_in(tmp.path(), src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "diags: {diags:?}"
    );
    let names = names_in(&store, "later_fns");
    // helper is at byte 29 of a.c (after `int main(void) { return 0; }\n`);
    // b.c's first int-decl is `worker` at byte 0 (LO=0, must drop).
    // main in b.c is at byte 31 (LO=31, must keep).
    assert!(
        names.contains(&"helper".to_string()),
        "helper (LO=29) should pass LO > 10: {names:?}"
    );
    assert!(
        !names.contains(&"worker".to_string()),
        "worker (LO=0) should fail LO > 10: {names:?}"
    );
}

// ── 5. Unparseable DSL → `lower/where-parse` diag ──────────────────

#[test]
fn where_unparseable_dsl_is_compile_diag() {
    // The backtick body parses fine at the surface CST. The Pratt
    // predicate parser then rejects `${NAME} =` because the rhs is
    // missing — emits `lower/where-parse`.
    let tmp = write_fixture();
    let src = r#"
        rule(:bad, NAME?) {
            fs(glob`**/*.c`) > read > re`int (?P<NAME>[a-z_]+)\(`
                > where`${NAME} =`
        };
    "#;
    let (_, diags) = run_in(tmp.path(), src);
    assert!(
        diags
            .iter()
            .any(|d| d.code.as_ref() == "lower/where-parse"),
        "expected a lower/where-parse diag, got: {:?}",
        diags.iter().map(|d| d.code.as_ref()).collect::<Vec<_>>()
    );
}

// ── 6. Fused-path predicate pushdown is unchanged ──────────────────

#[test]
fn where_still_fused_when_liftable() {
    // Pure RuleQuery + RuleQuery + where — the fuser must still lift
    // the predicate into JOIN ON. Mirrors fuser_kinds_target.rs's
    // `where_backtick_dsl_body_is_pushed_into_join_on_clause` but lives
    // here so this test file owns the regression check for its
    // sibling code path.
    use effect_runtime::v2::{FactStore, MemFactStore};
    use v4::compile::parse::host_parse;
    use v4::compile::walk::walk_program;
    use v4::lower::{default_registry, LowerCtx};
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits(FS?, LO?) > hits(FS?, OTHER_LO?) > where`${LO} != ${OTHER_LO}`
        }
    "#;
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {parse_diags:?}");
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let dir = std::env::temp_dir();
    let mut ctx = LowerCtx::new(store, dir);
    let (rules, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.iter().all(|d| d.severity != Severity::Error),
        "walk: {walk_diags:?}"
    );
    let pairs = rules
        .iter()
        .find(|r| r.name() == "hit_pairs")
        .expect("hit_pairs in fused rules");
    let sql = pairs
        .fused_sql()
        .expect("hit_pairs must be FullSql with fused SQL");
    let on_section = sql.split("WHERE").next().unwrap_or("");
    assert!(
        on_section.contains("<>") || on_section.contains("!="),
        "predicate must be pushed into JOIN ON, got: {sql}"
    );
}

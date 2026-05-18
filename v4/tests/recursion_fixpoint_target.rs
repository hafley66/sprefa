//! Surface test: a recursive sprf rule drives the fixpoint to a LEAST
//! FIXED POINT, not a single hop. GREEN as of the recursion wire.
//!
//! Engine layer (`v4::fixpoint::eval_stratum`) proves recursion→fixpoint
//! with a hand-built `EvalRule` (`tests/retraction_ph6.rs`). This pins
//! the junction: a PARSED `.sprf` recursive rule, in the canonical
//! C-spine two-overload form (base + self-referential step), routed
//! through the real run path transitively closes.
//!
//! What it took (the prior "3 stacked gaps incl. trigger" model was a
//! reserved-word confound; see v4/docs/v4-recursion-surface-gaps.md):
//!   - Gap 0: the fuser emitted UNQUOTED column idents — `r0.X AS FROM`
//!     is a SQLite syntax error; the spec's old `FROM`/`TO` columns
//!     masked everything as `reach=[]`. fuser now `qid()`-quotes.
//!   - Gap 2.5: the recursive step reads its OWN `{rule}_facts`; the
//!     run-path adapter excluded the target from source-view rewrite,
//!     so the self-read hit the raw FK table and failed. `run_fused_sql`
//!     now includes the self view for `is_recursive()` rules.
//!   - Gap 2: the FullSql seam ran each rule ONCE. `run_fused_sql` now
//!     loops the recursive INSERT to a fixed point (cap-guarded).
//!
//! Graph a→b→c→d. Full transitive closure (6 pairs):
//!   a-b b-c c-d  a-c b-d  a-d
//!
//! Asserted on the raw `reach` fact table. Columns are `SRC`/`DST`
//! (NOT `FROM`/`TO`): keeping a non-reserved pair here is the standing
//! regression guard that the quoting fix and the reserved-word class
//! both stay covered (the probe below pins the `FROM`-class directly).

use std::fs;
use std::sync::Arc;

use v4::app::{
    build_router, GetFactTableReq, InProcessClient, RunReq, SprfClient, SprfState,
};

#[tokio::test]
async fn recursive_rule_transitively_closes_to_fixed_point() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("reach.sprf");

    // Canonical C-spine recursion: two `reach` overloads. Base reads
    // `edge`; the step reads its own sink `reach` joined with `edge`.
    // `edge` is populated BEFORE the `reach` overloads: the FullSql
    // run path is single-pass in source order, so a rule reading a
    // relation filled by a later pipe would see it empty. Ordering
    // edge-first isolates the recursion→fixpoint gap (the wire under
    // test) from evaluation-order effects.
    let src = r#"
rule(:edge, X?, Y?);

`a|b
b|c
c|d`
  > split`
`
  > re`(?P<X>[^|]+)\|(?P<Y>.+)`
  > edge(X, Y);

rule(:reach, SRC?, DST?) { edge?(SRC?, DST?) };
rule(:reach, SRC?, DST?) { reach?(SRC?, MID?) > edge?(MID?, DST?) };
"#;
    fs::write(&sprf, src).unwrap();

    let fact_db = root.path().join("facts.db");
    let state = Arc::new(SprfState::new_with_sqlite_facts(
        root.path().to_path_buf(),
        &fact_db,
    ));
    let client = InProcessClient::new(build_router(state));
    client
        .run(RunReq {
            path: sprf.clone(),
            root: Some(root.path().to_path_buf()),
        })
        .await
        .expect("run ok");

    let reach = client
        .get_fact_table(GetFactTableReq {
            name: "reach".into(),
            limit: None,
        })
        .await
        .unwrap();

    let mut pairs: Vec<String> = reach
        .rows
        .iter()
        .map(|r| {
            let g = |k: &str| {
                r.fields
                    .iter()
                    .find(|(c, _)| c == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            format!("{}-{}", g("SRC"), g("DST"))
        })
        .collect();
    pairs.sort();
    pairs.dedup();

    let expected: Vec<&str> = vec!["a-b", "a-c", "a-d", "b-c", "b-d", "c-d"];
    assert_eq!(
        pairs.iter().map(String::as_str).collect::<Vec<_>>(),
        expected,
        "recursive `reach` must be the FULL transitive closure \
         (least fixed point), got {pairs:?}"
    );
}

// CONTRADICTION PROBE — settles whether a `rule(){body}` decl with no
// bare caller is evaluated at the run path AT ALL. Single NON-recursive
// base overload only; `edge` pre-filled before it; the body reads ONLY
// `edge` (immune to Gap 2.5, the self-`_facts` source-view exclusion).
//   reach == {a-b,b-c,c-d}  => decls DO run once; Gap 1 is query-trigger
//                              only; earlier reach=[] was Gap 2.5 + a
//                              separate base mask.
//   reach == []             => decls are SKIPPED at the run path; Gap 1
//                              is a real trigger gap; design it next.
#[tokio::test]
async fn probe_base_overload_decl_runs_at_run_path() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("probe.sprf");

    let src = r#"
rule(:edge, X?, Y?);

`a|b
b|c
c|d`
  > split`
`
  > re`(?P<X>[^|]+)\|(?P<Y>.+)`
  > edge(X, Y);

rule(:reach, SRC?, DST?) { edge?(SRC?, DST?) };
"#;
    fs::write(&sprf, src).unwrap();

    let fact_db = root.path().join("facts.db");
    let state = Arc::new(SprfState::new_with_sqlite_facts(
        root.path().to_path_buf(),
        &fact_db,
    ));
    let client = InProcessClient::new(build_router(state));
    client
        .run(RunReq {
            path: sprf.clone(),
            root: Some(root.path().to_path_buf()),
        })
        .await
        .expect("run ok");

    let dump = |name: &str| {
        let c = &client;
        let n = name.to_string();
        async move {
            let t = c
                .get_fact_table(GetFactTableReq {
                    name: n,
                    limit: None,
                })
                .await
                .unwrap();
            let mut p: Vec<String> = t
                .rows
                .iter()
                .map(|r| {
                    let g = |k: &str| {
                        r.fields
                            .iter()
                            .find(|(c, _)| c == k)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default()
                    };
                    format!("{}-{}", g("X"), g("Y"))
                })
                .collect();
            p.sort();
            p
        }
    };
    let edge = dump("edge").await;

    let reach = client
        .get_fact_table(GetFactTableReq {
            name: "reach".into(),
            limit: None,
        })
        .await
        .unwrap();
    let mut rpairs: Vec<String> = reach
        .rows
        .iter()
        .map(|r| {
            let g = |k: &str| {
                r.fields
                    .iter()
                    .find(|(c, _)| c == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            format!("{}-{}", g("SRC"), g("DST"))
        })
        .collect();
    rpairs.sort();

    eprintln!("PROBE edge  = {edge:?}");
    eprintln!("PROBE reach = {rpairs:?}");
    assert_eq!(
        edge,
        vec!["a-b", "b-c", "c-d"],
        "edge must be filled (probe precondition)"
    );
    assert_eq!(
        rpairs,
        vec!["a-b", "b-c", "c-d"],
        "VERDICT: base-overload decl did NOT run at the run path \
         (Gap 1 is a real trigger gap), got {rpairs:?}"
    );
}

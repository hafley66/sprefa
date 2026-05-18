//! RED spec (Part 2): retract a base `edge` fact, recursive `reach`
//! must follow to the new least fixed point.
//!
//! Graph run 1: a->b->c->d  =>  reach = all 6 pairs.
//! Run 2 (same fact-db): `b|c` removed from source.
//! SOUND DRed result: edge = {a-b, c-d}, reach = {a-b, c-d}
//! (a-c, b-c, b-d, a-d all lose their only derivation path).
//!
//! Observed today (see v4/docs/v4-recursion-surface-gaps.md
//! "Retraction gaps"): reach is materialize-forward. The recursive
//! FullSql loop in `run_fused_sql` is `INSERT OR IGNORE` only with no
//! link to `mounted_query_support` / `cascade_retract`, so no closure
//! row is ever retracted. This spec also rides on the Part-1 gap
//! (`surface_owner_retract_target.rs`): the base `edge` write is
//! insert-only too, so `edge(b,c)` itself does not retract. Both must
//! be wired for this to go GREEN.
//!
//! `#[ignore]`'d: asserts the SOUND target, not today's behavior, so
//! the gate stays green while the gap is pinned.

use std::fs;
use std::sync::Arc;

use v4::app::{
    build_router, GetFactTableReq, InProcessClient, RunReq, SprfClient, SprfState,
};

fn pairs(t: &v4::app::FactTable, a: &str, b: &str) -> Vec<String> {
    let mut v: Vec<String> = t
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
            format!("{}-{}", g(a), g(b))
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

#[ignore = "RED spec: recursive reach is materialize-forward (no DRed \
            link); see v4/docs/v4-recursion-surface-gaps.md"]
#[tokio::test]
async fn retract_base_edge_cascades_recursive_reach() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("g.sprf");
    let fact_db = root.path().join("facts.db");

    let state = Arc::new(SprfState::new_with_sqlite_facts(
        root.path().to_path_buf(),
        &fact_db,
    ));
    let client = InProcessClient::new(build_router(state));

    let prog = |edges: &str| {
        format!(
            r#"
rule(:edge, X?, Y?);

`{edges}`
  > split`
`
  > re`(?P<X>[^|]+)\|(?P<Y>.+)`
  > edge(X, Y);

rule(:reach, SRC?, DST?) {{ edge?(SRC?, DST?) }};
rule(:reach, SRC?, DST?) {{ reach?(SRC?, MID?) > edge?(MID?, DST?) }};
"#
        )
    };

    fs::write(&sprf, prog("a|b\nb|c\nc|d")).unwrap();
    client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .expect("run1 ok");
    let r1 = pairs(
        &client
            .get_fact_table(GetFactTableReq { name: "reach".into(), limit: None })
            .await
            .unwrap(),
        "SRC",
        "DST",
    );
    assert_eq!(
        r1,
        vec!["a-b", "a-c", "a-d", "b-c", "b-d", "c-d"],
        "run1 must close fully"
    );

    fs::write(&sprf, prog("a|b\nc|d")).unwrap();
    client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .expect("run2 ok");

    let e2 = pairs(
        &client
            .get_fact_table(GetFactTableReq { name: "edge".into(), limit: None })
            .await
            .unwrap(),
        "X",
        "Y",
    );
    let r2 = pairs(
        &client
            .get_fact_table(GetFactTableReq { name: "reach".into(), limit: None })
            .await
            .unwrap(),
        "SRC",
        "DST",
    );
    eprintln!("RUN2 edge  = {e2:?}");
    eprintln!("RUN2 reach = {r2:?}");

    assert_eq!(e2, vec!["a-b", "c-d"], "edge(b,c) must retract (Part 1)");
    assert_eq!(
        r2,
        vec!["a-b", "c-d"],
        "recursive reach must re-close to the new least fixed point \
         after the base retraction (Part 2)"
    );
}

//! RED spec (Part 1): owner-scoped retraction for a stream-fed rule
//! write. No recursion — isolates the surface-retraction gap.
//!
//! `` `a|b\nb|c\nc|d` > split > re > edge(X,Y) `` then a re-run with
//! `b|c` removed from the source. A sound owner-scoped reconcile would
//! prune the row the owner no longer produces:
//!   run1 edge = {a-b, b-c, c-d}
//!   run2 edge = {a-b, c-d}
//!
//! Observed today: `edge(X,Y)` lowers to a plain `FactWrite`
//! (`sql.rs` -> `fact.rs`). `FactWrite::render_batch` records DRed
//! support rows ONLY for input cursors carrying
//! `mounted_query::SUPPORT_CURSOR_ID` (fact.rs:101-104); a literal/
//! stream head does not set it, so `store.insert_batch` (fact.rs:125)
//! is insert-only with no generation/owner reconcile. `edge(b,c)`
//! persists. Owner-scoped retraction is wired only for the
//! `fs>read>re` hits-owner path and the mounted SQL-query support
//! path, not for stream/literal pipes terminating in a rule write.
//!
//! `#[ignore]`'d: asserts the SOUND target so the gate stays green
//! while the gap is pinned.

use std::fs;
use std::sync::Arc;

use v4::app::{
    build_router, GetFactTableReq, InProcessClient, RunReq, SprfClient, SprfState,
};

fn pairs(t: &v4::app::FactTable) -> Vec<String> {
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
            format!("{}-{}", g("X"), g("Y"))
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

#[ignore = "RED spec: stream-fed FactWrite is insert-only, no \
            owner-scoped retraction; see v4/docs/v4-recursion-surface-gaps.md"]
#[tokio::test]
async fn rerun_with_dropped_source_row_retracts_owner_fact() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("e.sprf");
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
"#
        )
    };

    fs::write(&sprf, prog("a|b\nb|c\nc|d")).unwrap();
    client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .expect("run1 ok");
    let e1 = pairs(
        &client
            .get_fact_table(GetFactTableReq { name: "edge".into(), limit: None })
            .await
            .unwrap(),
    );
    assert_eq!(e1, vec!["a-b", "b-c", "c-d"], "run1 fills all 3 edges");

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
    );
    eprintln!("RUN2 edge = {e2:?}");

    assert_eq!(
        e2,
        vec!["a-b", "c-d"],
        "owner-scoped reconcile must drop edge(b,c) once the source \
         row that produced it is gone"
    );
}

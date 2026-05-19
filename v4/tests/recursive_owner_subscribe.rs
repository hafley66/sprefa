//! Verification for the recursive-owner-subscribe wire.
//!
//! A recursive `reach` rule over a LITERAL-block `edge` relation (a
//! literal head ⇒ `full_extent` ⇒ Part-1 owner reconcile refreshes the
//! base each run deterministically, independent of any fs corpus
//! oracle). Three runs share one persistent `--fact-db`; each run is a
//! FRESH state + router (only the fact-db file persists), exactly how
//! successive `sprefa-run` invocations behave.
//!
//! Asserts the SOUND DEFAULT behavior (the run-scoped incremental skip
//! is opt-in behind `SPREFA_REC_INCREMENTAL`; default = recompute, the
//! proven Part-2 path — the available oracle is a process-wide corpus
//! signal, not per-rule source attribution, so skipping by default is
//! unsound):
//!   run1 cold      : closure converges (6 pairs over a->b->c->d).
//!   run2 unchanged : recompute ⇒ closure unchanged == run1 (the
//!                    structural owner/subscribe/stratify wire above is
//!                    unconditional and must not corrupt the result).
//!   run3 edited    : an edge appended ⇒ recompute ⇒ closure grows to
//!                    the new least fixed point.
//!
//! Exercises (no panic / no hang) the unconditional structural wire:
//! per-rule `declare_owner` + `subscribe` to the external `edge` table
//! source, the self/co-recursive wake-cycle guard, the `stratify` Ok
//! path. Retraction-on-edit of a recursive rule is covered separately
//! by `retract_recursion_demo.rs` (literal head ⇒ Part-1 owner
//! reconcile; an fs-fed base is `full_extent=false` by design).

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

#[tokio::test]
async fn recursive_owner_wire_recomputes_soundly_across_runs() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("g.sprf");
    let fact_db = root.path().join("facts.db");

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

    // Each run is a FRESH state + router sharing only the persistent
    // `--fact-db` file — exactly how successive `sprefa-run` invocations
    // work. The literal head re-renders deterministically every run, so
    // the recompute is observable without depending on the fs corpus
    // oracle.
    macro_rules! run_reach {
        ($edges:expr) => {{
            fs::write(&sprf, prog($edges)).unwrap();
            let client = InProcessClient::new(build_router(Arc::new(
                SprfState::new_with_sqlite_facts(
                    root.path().to_path_buf(),
                    &fact_db,
                ),
            )));
            client
                .run(RunReq {
                    path: sprf.clone(),
                    root: Some(root.path().to_path_buf()),
                })
                .await
                .expect("run ok");
            pairs(
                &client
                    .get_fact_table(GetFactTableReq {
                        name: "reach".into(),
                        limit: None,
                    })
                    .await
                    .unwrap(),
                "SRC",
                "DST",
            )
        }};
    }

    // run1 cold: full closure.
    let r1 = run_reach!("a|b\nb|c\nc|d");
    assert_eq!(
        r1,
        vec!["a-b", "a-c", "a-d", "b-c", "b-d", "c-d"],
        "run1 cold must close fully"
    );

    // run2 source unchanged: recompute (sound default). The structural
    // owner/subscribe/stratify wire must not corrupt the fixed point —
    // closure stays EXACTLY run1's.
    let r2 = run_reach!("a|b\nb|c\nc|d");
    assert_eq!(r2, r1, "run2 unchanged must equal run1 (sound recompute)");

    // run3 source edited (`d|e` appended): recompute ⇒ closure grows to
    // include everything reachable through the new edge.
    let r3 = run_reach!("a|b\nb|c\nc|d\nd|e");
    assert_eq!(
        r3,
        vec![
            "a-b", "a-c", "a-d", "a-e", "b-c", "b-d", "b-e", "c-d", "c-e",
            "d-e"
        ],
        "run3 changed must recompute (not skip): closure grows through d->e"
    );
}

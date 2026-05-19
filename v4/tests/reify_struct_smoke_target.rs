//! RED slice for the `reify` macro op (smallest vertical).
//!
//! Pins the whole reify contract in ONE fixture: a Rust struct is
//! down-converted to a sprf `rule(:Type, field?: t.ty, …)` decl,
//! written into a managed comment region in the .sprf file itself
//! immediately after the `reify` statement, then EXPANDED + RUN as
//! real sprf within the same `sprefa-run` (a true macro).
//!
//! Fixture on disk:
//!   <root>/src/geom.rs  =  `pub struct Point { x: i64, y: i64 }`
//!   <root>/g.sprf       =  (1) a reify statement
//!                          (2) a literal write into `Point`
//!
//! `Point` is NEVER declared by hand. The ONLY thing that can make
//! `Point(x, y)` a legal write is the reify macro having expanded a
//! `rule(:Point, x?: t.i64, y?: t.i64);` decl into the file ABOVE the
//! write, before the normal parse/walk/lower pipeline ran. So one
//! row in `Point` proves: region written + correct types + macro
//! expanded + ordered before its reader.
//!
//! Pinned contract (chosen defaults — redirectable):
//!   op name      : `reify`
//!   surface      : `reify(:rs)`kind: struct_item`;`
//!   region marks : `#<reify gen h=…>` … `#</reify gen>`
//!   type tokens  : retained as a `# reify types:` comment (v0 has no
//!                  typed-column grammar; structural decl only)
//!
//! Pins:
//!   1. after run, g.sprf on disk contains the marked region with a
//!      valid `rule(:Point, x?, y?);` decl + a `# reify types:` line.
//!   2. the `Point` fact table has the row (x=1, y=2)  ⇒ the macro
//!      expanded a live decl, ordered before the hand-written write.
//!   3. a SECOND run is idempotent: region byte-identical (stable
//!      hash, no duplicate region) and `Point` still exactly one row
//!      (no double-decl error from a re-emitted region).
//!
//! RED today: there is no `reify` op, so the statement is an unknown
//! op, no region is written, `Point` is undeclared, the write fails,
//! all three pins fail. GREEN = the op + crate + macro pre-pass.

use std::fs;
use std::sync::Arc;

use v4::app::{
    build_router, GetFactTableReq, InProcessClient, RunReq, SprfClient, SprfState,
};

const GEOM_RS: &str = "pub struct Point { x: i64, y: i64 }\n";

// (1) reify, then (2) a hand write into the rule reify must generate.
const PROG: &str = "reify(:rs)`kind: struct_item`;\n\
`1|2`\n  > split`\n`\n  > re`(?P<x>[^|]+)\\|(?P<y>.+)`\n  > Point(x, y);\n";

async fn run_once(root: &std::path::Path, sprf: &std::path::Path) -> InProcessClient {
    let fact_db = root.join("facts.db");
    let client = InProcessClient::new(build_router(Arc::new(
        SprfState::new_with_sqlite_facts(root.to_path_buf(), &fact_db),
    )));
    client
        .run(RunReq {
            path: sprf.to_path_buf(),
            root: Some(root.to_path_buf()),
        })
        .await
        .expect("run ok");
    client
}

fn point_rows(t: &v4::app::FactTable) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = t
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
            (g("x"), g("y"))
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

#[tokio::test]
async fn reify_struct_writes_region_and_macro_expands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/geom.rs"), GEOM_RS).unwrap();
    let sprf = root.join("g.sprf");
    fs::write(&sprf, PROG).unwrap();

    // ---- run 1 ----
    let c1 = run_once(root, &sprf).await;

    // pin 1: the file grew a marked region with the generated decl.
    let after = fs::read_to_string(&sprf).unwrap();
    assert!(
        after.contains("#<reify gen") && after.contains("#</reify gen>"),
        "g.sprf must contain a reify-managed region after run; got:\n{after}"
    );
    // v0: a VALID structural decl + the extracted types retained in a
    // trailing comment (sprf has no `col?: type` grammar yet; typed
    // columns are the separate value-space-types initiative).
    assert!(
        after.contains("rule(:Point, x?, y?);"),
        "region must declare Point structurally; got:\n{after}"
    );
    assert!(
        after.contains("# reify types: x: t.i64, y: t.i64"),
        "region must retain extracted types as a comment; got:\n{after}"
    );

    // pin 2: the macro expanded a LIVE decl, ordered before the write.
    let t1 = c1
        .get_fact_table(GetFactTableReq {
            name: "Point".into(),
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(
        point_rows(&t1),
        vec![("1".to_string(), "2".to_string())],
        "Point(1,2) must exist ⇒ reify decl expanded + ordered before its writer"
    );

    // pin 3: idempotent re-run (region stable, no duplicate decl).
    let region_after_1 = after.clone();
    let c2 = run_once(root, &sprf).await;
    let after_2 = fs::read_to_string(&sprf).unwrap();
    assert_eq!(
        after_2, region_after_1,
        "second run must be byte-idempotent (stable hash, no re-emit)"
    );
    let t2 = c2
        .get_fact_table(GetFactTableReq {
            name: "Point".into(),
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(
        point_rows(&t2),
        vec![("1".to_string(), "2".to_string())],
        "re-run must not duplicate the region/decl"
    );
}

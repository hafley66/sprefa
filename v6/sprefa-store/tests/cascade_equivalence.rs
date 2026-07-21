//! Lab: the incremental weight cascade computes the SAME survivor set as a
//! from-scratch anti-join recompute. This pins the equivalence the book claims —
//! the `weight` column is a cache of "has a surviving support", nothing more.
//!
//! Method A (incremental, O(delta)): decrement weights, cascade the zeros.
//! Method B (recompute, O(edges), the oracle): a row is alive iff it is
//!   reachable from a surviving root over the ORIGINAL support edges — i.e. it
//!   still has at least one live support transitively. Pure anti-join /
//!   reachability, no weights.
//! The two survivor sets must be identical for ANY graph.

use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use sprefa_store::cascade;

const W: i64 = 2_000;

async fn exec(db: &DatabaseConnection, sql: &str) {
    db.execute_unprepared(sql).await.unwrap();
}

async fn scalar(db: &DatabaseConnection, sql: &str) -> i64 {
    db.query_one_raw(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        sql.to_owned(),
    ))
    .await
    .unwrap()
    .unwrap()
    .try_get_by_index::<i64>(0)
    .unwrap()
}

#[tokio::test]
async fn incremental_cascade_equals_from_scratch_recompute() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    cascade::create_schema(&db).await.unwrap();

    // ---- a non-trivial mixed-support DAG ------------------------------------
    // roots r0=(0,0), r1=(0,1)
    // L1 (tag1): parent r0 always; ALSO r1 when j%3==0  -> mixed single/dual support
    // L2 (tag2): parents L1[j] and L1[(j+1)%W]          -> two derived parents
    // L3 (tag3): parent L2[j]                            -> single support
    let mut rows: Vec<(i64, i64, i64)> = vec![(0, 0, 1), (0, 1, 1)];
    let mut deps: Vec<(i64, i64, i64, i64)> = Vec::new();
    for j in 0..W {
        let l1_supports = if j % 3 == 0 { 2 } else { 1 };
        rows.push((1, j, l1_supports));
        rows.push((2, j, 2));
        rows.push((3, j, 1));
        deps.push((0, 0, 1, j)); // r0 -> L1[j]
        if j % 3 == 0 {
            deps.push((0, 1, 1, j)); // r1 -> L1[j]
        }
        deps.push((1, j, 2, j)); // L1[j] -> L2[j]
        deps.push((1, (j + 1) % W, 2, j)); // L1[j+1] -> L2[j]
        deps.push((2, j, 3, j)); // L2[j] -> L3[j]
    }
    cascade::insert_rows(&db, &rows).await.unwrap();
    cascade::insert_deps(&db, &deps).await.unwrap();

    // snapshot the ORIGINAL edges before the cascade mutates cx_dep
    exec(&db, "CREATE TABLE lab_orig_dep AS SELECT * FROM cx_dep").await;

    // ---- Method A: incremental cascade of retracting r0 ---------------------
    cascade::retract(&db, &[(0, 0)]).await.unwrap();
    // survivors_A = rows still at weight > 0 (soft-delete: dead rows are retained
    // at weight <= 0, not physically removed).

    // ---- Method B: from-scratch reachability from surviving roots -----------
    // surviving roots = original roots (never a child) minus the retracted r0.
    // cx_dep is now single-key (parent_key,child_key); r0 = (0,0) encodes to key 0.
    exec(
        &db,
        "CREATE TABLE lab_alive AS
         WITH RECURSIVE alive(key) AS (
            SELECT DISTINCT d.parent_key
              FROM lab_orig_dep d
             WHERE d.parent_key NOT IN (SELECT child_key FROM lab_orig_dep)
               AND d.parent_key <> 0
            UNION
            SELECT d.child_key
              FROM lab_orig_dep d JOIN alive a
                ON d.parent_key = a.key
         )
         SELECT key FROM alive",
    )
    .await;

    // ---- the two survivor sets must be identical ----------------------------
    let a_count = scalar(&db, "SELECT count(*) FROM cx_row WHERE weight > 0").await;
    let b_count = scalar(&db, "SELECT count(*) FROM lab_alive").await;
    let a_not_b = scalar(
        &db,
        "SELECT count(*) FROM (SELECT key FROM cx_row WHERE weight > 0 EXCEPT SELECT key FROM lab_alive)",
    )
    .await;
    let b_not_a = scalar(
        &db,
        "SELECT count(*) FROM (SELECT key FROM lab_alive EXCEPT SELECT key FROM cx_row WHERE weight > 0)",
    )
    .await;

    assert_eq!(a_not_b, 0, "incremental kept rows the recompute would have dropped");
    assert_eq!(b_not_a, 0, "incremental dropped rows the recompute would have kept");
    assert_eq!(a_count, b_count, "same survivor count");
    // and it is a real graph, not everything-lives / everything-dies
    let total = 2 + 3 * W;
    assert!(a_count > 0 && a_count < total, "a non-trivial subset survived: {a_count}/{total}");
    eprintln!(
        "[equiv] incremental cascade == anti-join recompute: {a_count}/{total} survive, symmetric diff 0"
    );
}

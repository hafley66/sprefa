//! ONE large golden test for the isolated manual cascade. Builds a wide DAG at
//! scale where some rows have TWO supports, retracts one root, and proves:
//!   - the correct transitive set dies, survivors with alternate support live
//!   - retraction = subtraction (weight-2 rows drop to 1 and survive)
//!   - the cascade is O(depth) statements, not O(rows), at 100k rows
//!
//! Topology (tags are the polymorphic "table" of the (tag,id) FK):
//!   tag 0: two roots  r0=(0,0), r1=(0,1)
//!   tag 1: layer A, WIDTH nodes. EVEN j depend on BOTH roots (weight 2);
//!          ODD j depend on r0 only (weight 1).
//!   tag 2: layer B, WIDTH nodes. B[j] depends on A[j] (weight 1).
//! Retract r0:
//!   r0 dies -> every A loses 1: ODD A (w1->0) die, EVEN A (w2->1) survive
//!           -> B under dead ODD A (w1->0) die, B under surviving EVEN A live.

use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use sprefa_store::cascade;
use sprefa_store::stmt_counter;
use std::time::Instant;

const WIDTH: usize = 50_000;

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
async fn manual_cascade_retracts_by_subtraction_at_scale() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .await
        .unwrap();
    cascade::create_schema(&db).await.unwrap();

    // ---- build the DAG -------------------------------------------------------
    let t = Instant::now();
    let mut rows: Vec<(i64, i64, i64)> = vec![(0, 0, 1), (0, 1, 1)]; // r0, r1
    let mut deps: Vec<(i64, i64, i64, i64)> = Vec::new();
    for j in 0..WIDTH as i64 {
        let even = j % 2 == 0;
        rows.push((1, j, if even { 2 } else { 1 })); // A[j]
        rows.push((2, j, 1)); // B[j]
        deps.push((0, 0, 1, j)); // r0 -> A[j]
        if even {
            deps.push((0, 1, 1, j)); // r1 -> A[j]  (second support)
        }
        deps.push((1, j, 2, j)); // A[j] -> B[j]
    }
    cascade::insert_rows(&db, &rows).await.unwrap();
    cascade::insert_deps(&db, &deps).await.unwrap();
    let total = 2 + 2 * WIDTH as i64;
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row").await, total);
    eprintln!(
        "[build] {} rows, {} deps in {:?}",
        total,
        deps.len(),
        t.elapsed()
    );

    // ---- retract r0, counting statements ------------------------------------
    let t = Instant::now();
    stmt_counter::reset();
    let rounds = cascade::retract(&db, &[(0, 0)]).await.unwrap();
    let stmts = stmt_counter::get();
    eprintln!(
        "[retract] r0 -> {} rounds, {} statements, {:?}",
        rounds,
        stmts,
        t.elapsed()
    );

    // ---- the cascade set is exactly right -----------------------------------
    // Soft-delete contract: dead rows STAY at weight <= 0; survivors are weight > 0.
    let survivors = scalar(&db, "SELECT count(*) FROM cx_row WHERE weight > 0").await;
    // dead = r0 + ODD A (WIDTH/2) + ODD B (WIDTH/2); survivors = the rest
    let expected_dead = 1 + (WIDTH / 2) as i64 + (WIDTH / 2) as i64;
    assert_eq!(survivors, total - expected_dead, "exact transitive set died");

    // r0 dead, r1 alive
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row WHERE tag=0 AND id=0 AND weight>0").await, 0);
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row WHERE tag=0 AND id=1 AND weight>0").await, 1);
    // ODD A died; EVEN A survived AT WEIGHT 1 (2 - 1) — retraction = subtraction
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row WHERE tag=1 AND id=1 AND weight>0").await, 0);
    assert_eq!(scalar(&db, "SELECT weight FROM cx_row WHERE tag=1 AND id=0").await, 1);
    // B under dead ODD A died; B under surviving EVEN A lived
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row WHERE tag=2 AND id=1 AND weight>0").await, 0);
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row WHERE tag=2 AND id=0 AND weight>0").await, 1);

    // ---- it is O(depth), not O(rows) ----------------------------------------
    assert_eq!(rounds, 3, "root -> layer A -> layer B");
    assert!(
        stmts < 40,
        "cascade over {total} rows cost {stmts} statements (O(depth)); an N+1 cascade would be ~{expected_dead}"
    );

    // ---- soft-delete invariant: dead rows are RETAINED (weight <= 0), not
    //      physically deleted. Every dead row still sits in cx_row; nothing was
    //      DELETEd during the cascade (that is the speed win).
    let dead_retained = scalar(&db, "SELECT count(*) FROM cx_row WHERE weight <= 0").await;
    assert_eq!(dead_retained, expected_dead, "dead rows retained, not deleted");
    assert_eq!(scalar(&db, "SELECT count(*) FROM cx_row").await, total, "no row physically removed");
    eprintln!("[ok] survivors={survivors}, dead retained={dead_retained}");
}

//! Datalog / relational-algebra operation coverage for the on-disk (SQLite)
//! candidate, each flexed at LOW volume against an independent Rust oracle. This
//! is the "join / union / negation / aggregation / recursion dance" sprefa needs.
//! Correctness only here; scale is a separate pass. Every test builds a tiny fact
//! set, runs the operation as SQL, and asserts byte-equality with a hand-written
//! oracle computed from the same facts.

use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

async fn mem() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON;").await.unwrap();
    db
}

async fn exec(db: &DatabaseConnection, sql: &str) {
    db.execute_unprepared(sql).await.unwrap();
}

/// Run a query and collect its rows as `Vec<Vec<i64>>` (all columns i64).
async fn rows(db: &DatabaseConnection, sql: &str, cols: usize) -> Vec<Vec<i64>> {
    let out = db
        .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await
        .unwrap();
    let mut v: Vec<Vec<i64>> = out
        .iter()
        .map(|r| (0..cols).map(|i| r.try_get_by_index::<i64>(i).unwrap()).collect())
        .collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<Vec<i64>>) -> Vec<Vec<i64>> {
    v.sort();
    v
}

// ---- 1. two-way JOIN ------------------------------------------------------
// call(caller,callee) JOIN def(fn,file) ON callee=fn  -> (caller,callee,file)
#[tokio::test]
async fn two_way_join() {
    let db = mem().await;
    exec(&db, "CREATE TABLE call(caller INTEGER, callee INTEGER);
               CREATE TABLE def(fn INTEGER, file INTEGER);
               INSERT INTO call VALUES (1,2),(1,3),(2,3);
               INSERT INTO def  VALUES (2,10),(3,11);").await;
    let got = rows(&db,
        "SELECT c.caller, c.callee, d.file FROM call c JOIN def d ON c.callee = d.fn", 3).await;

    let call = [(1, 2), (1, 3), (2, 3)];
    let def = [(2, 10), (3, 11)];
    let mut oracle = Vec::new();
    for (caller, callee) in call {
        for (fnn, file) in def {
            if callee == fnn { oracle.push(vec![caller, callee, file]); }
        }
    }
    assert_eq!(got, sorted(oracle));
}

// ---- 2. UNION + DISTINCT --------------------------------------------------
#[tokio::test]
async fn union_distinct() {
    let db = mem().await;
    exec(&db, "CREATE TABLE e1(a INTEGER, b INTEGER);
               CREATE TABLE e2(a INTEGER, b INTEGER);
               INSERT INTO e1 VALUES (1,2),(2,3);
               INSERT INTO e2 VALUES (2,3),(3,4);").await;
    let got = rows(&db, "SELECT a,b FROM e1 UNION SELECT a,b FROM e2", 2).await;

    use std::collections::BTreeSet;
    let mut set: BTreeSet<(i64, i64)> = BTreeSet::new();
    for e in [(1, 2), (2, 3)] { set.insert(e); }
    for e in [(2, 3), (3, 4)] { set.insert(e); }
    let oracle: Vec<Vec<i64>> = set.into_iter().map(|(a, b)| vec![a, b]).collect();
    assert_eq!(got, oracle);
}

// ---- 3. ANTIJOIN / stratified NEGATION ------------------------------------
// uncalled functions = def.fn that appear in NO call.callee ("dead code").
#[tokio::test]
async fn antijoin_negation() {
    let db = mem().await;
    exec(&db, "CREATE TABLE call(caller INTEGER, callee INTEGER);
               CREATE TABLE def(fn INTEGER);
               INSERT INTO call VALUES (1,2),(2,3);
               INSERT INTO def  VALUES (1),(2),(3),(4);").await;
    let got = rows(&db,
        "SELECT fn FROM def WHERE fn NOT IN (SELECT callee FROM call) ORDER BY fn", 1).await;

    use std::collections::BTreeSet;
    let called: BTreeSet<i64> = [2, 3].into_iter().collect();
    let oracle: Vec<Vec<i64>> =
        [1, 2, 3, 4].into_iter().filter(|f| !called.contains(f)).map(|f| vec![f]).collect();
    assert_eq!(got, sorted(oracle)); // expect {1,4}
    assert_eq!(got, vec![vec![1], vec![4]]);
}

// ---- 4. AGGREGATION: GROUP BY + COUNT -------------------------------------
// fan-in per callee = number of distinct callers.
#[tokio::test]
async fn aggregation_group_count() {
    let db = mem().await;
    exec(&db, "CREATE TABLE call(caller INTEGER, callee INTEGER);
               INSERT INTO call VALUES (1,3),(2,3),(1,4),(5,3);").await;
    let got = rows(&db,
        "SELECT callee, count(*) FROM call GROUP BY callee ORDER BY callee", 2).await;

    use std::collections::BTreeMap;
    let mut m: BTreeMap<i64, i64> = BTreeMap::new();
    for (_caller, callee) in [(1, 3), (2, 3), (1, 4), (5, 3)] { *m.entry(callee).or_insert(0) += 1; }
    let oracle: Vec<Vec<i64>> = m.into_iter().map(|(c, n)| vec![c, n]).collect();
    assert_eq!(got, oracle); // callee 3 -> 3, callee 4 -> 1
}

// ---- 5. THREE-WAY JOIN ----------------------------------------------------
// caller -> callee (call), callee lives in module (in_mod), module owned by team.
#[tokio::test]
async fn three_way_join() {
    let db = mem().await;
    exec(&db, "CREATE TABLE call(caller INTEGER, callee INTEGER);
               CREATE TABLE in_mod(fn INTEGER, module INTEGER);
               CREATE TABLE owns(module INTEGER, team INTEGER);
               INSERT INTO call VALUES (1,2),(1,3);
               INSERT INTO in_mod VALUES (2,20),(3,30);
               INSERT INTO owns VALUES (20,200),(30,300);").await;
    let got = rows(&db,
        "SELECT c.caller, o.team FROM call c \
         JOIN in_mod m ON c.callee=m.fn JOIN owns o ON m.module=o.module", 2).await;

    let oracle = sorted(vec![vec![1, 200], vec![1, 300]]);
    assert_eq!(got, oracle);
}

// ---- 6. RECURSION: transitive closure (WITH RECURSIVE) --------------------
#[tokio::test]
async fn recursion_transitive_closure() {
    let db = mem().await;
    exec(&db, "CREATE TABLE edge(src INTEGER, dst INTEGER);
               INSERT INTO edge VALUES (1,2),(2,3),(3,4),(1,5);").await;
    let got = rows(&db,
        "WITH RECURSIVE reach(src,dst) AS (
            SELECT src,dst FROM edge
            UNION
            SELECT r.src, e.dst FROM reach r JOIN edge e ON r.dst = e.src)
         SELECT src,dst FROM reach", 2).await;

    // oracle: BFS closure
    let edges = [(1, 2), (2, 3), (3, 4), (1, 5)];
    use std::collections::BTreeSet;
    let mut pairs: BTreeSet<(i64, i64)> = edges.iter().copied().collect();
    loop {
        let mut added = false;
        let snapshot: Vec<(i64, i64)> = pairs.iter().copied().collect();
        for &(a, b) in &snapshot {
            for &(c, d) in &edges {
                if b == c && pairs.insert((a, d)) { added = true; }
            }
        }
        if !added { break; }
    }
    let oracle: Vec<Vec<i64>> = pairs.into_iter().map(|(a, b)| vec![a, b]).collect();
    assert_eq!(got, oracle);
}

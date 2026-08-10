use crate::checksum;
use rusqlite::Connection;

pub struct EngineResult {
    pub derived: u64,
    pub checksum: u64,
    pub rounds: u64,
    pub statements: u64,
}

pub struct Measures {
    pub load_ms: u128,
    pub fixpoint_ms: u128,
    pub fold_ms: u128,
}

fn open_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    conn.pragma_update(None, "page_size", 16384).unwrap();
    conn.pragma_update(None, "temp_store", "MEMORY").unwrap();
    // :memory: has no journal/sync/cache to tune; these are measured no-ops.
    conn
}

fn create_edge_and_load(conn: &Connection, edges: &[(u32, u32)]) {
    conn.execute_batch(
        "CREATE TABLE edge (
            source INTEGER NOT NULL,
            target INTEGER NOT NULL,
            PRIMARY KEY (source, target)
        ) WITHOUT ROWID;",
    )
    .unwrap();
    conn.execute_batch("BEGIN;").unwrap();
    {
        let mut stmt = conn
            .prepare("INSERT OR IGNORE INTO edge (source, target) VALUES (?1, ?2)")
            .unwrap();
        for (source, target) in edges {
            stmt.execute((*source as i64, *target as i64)).unwrap();
        }
    }
    conn.execute_batch("COMMIT;").unwrap();
}

fn fold_reachable(conn: &Connection) -> u64 {
    let mut stmt = conn
        .prepare("SELECT source, target FROM reachable")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? as u32))
        })
        .unwrap();
    checksum::fold(rows.map(|r| r.unwrap()))
}

pub fn run_naive(edges: &[(u32, u32)]) -> (EngineResult, Measures) {
    let started = std::time::Instant::now();
    let conn = open_db();
    create_edge_and_load(&conn, edges);
    let load_ms = started.elapsed().as_millis();

    let conn = conn; // keep
    let fp_started = std::time::Instant::now();
    conn.execute_batch(
        "CREATE TABLE reachable (
            source INTEGER NOT NULL,
            target INTEGER NOT NULL,
            PRIMARY KEY (source, target)
        ) WITHOUT ROWID;",
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO reachable (source, target)
         WITH RECURSIVE closure (source, target) AS (
            SELECT source, target FROM edge
            UNION
            SELECT closure.source, edge.target
            FROM closure JOIN edge ON edge.source = closure.target
         )
         SELECT source, target FROM closure;",
    )
    .unwrap();
    let fixpoint_ms = fp_started.elapsed().as_millis();

    let fold_started = std::time::Instant::now();
    let derived: i64 = conn
        .query_row("SELECT count(*) FROM reachable", [], |row| row.get(0))
        .unwrap();
    let checksum = fold_reachable(&conn);
    let fold_ms = fold_started.elapsed().as_millis();
    (
        EngineResult {
            derived: derived as u64,
            checksum,
            rounds: 1,
            statements: 1,
        },
        Measures {
            load_ms,
            fixpoint_ms,
            fold_ms,
        },
    )
}

pub fn run_tuned_range(edges: &[(u32, u32)]) -> (EngineResult, Measures) {
    let started = std::time::Instant::now();
    let conn = open_db();
    create_edge_and_load(&conn, edges);
    let load_ms = started.elapsed().as_millis();

    conn.execute_batch(
        "CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL);
         CREATE UNIQUE INDEX reachable_pair ON reachable (source, target);",
    )
    .unwrap();

    let fp_started = std::time::Instant::now();
    let mut rounds = 0u64;
    let mut statements = 0u64;
    conn.execute_batch("BEGIN;").unwrap();
    {
        let mut seed = conn
            .prepare(
                "INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM edge",
            )
            .unwrap();
        let mut step = conn
            .prepare(
                "INSERT OR IGNORE INTO reachable (source, target)
                 SELECT known.source, edge.target
                 FROM reachable known JOIN edge ON edge.source = known.target
                 WHERE known.rowid BETWEEN ?1 AND ?2",
            )
            .unwrap();
        let mut low: i64 = 1;
        let mut high: i64 = seed.execute([]).unwrap() as i64;
        statements += 1;
        loop {
            let derived = step.execute((low, high)).unwrap() as i64;
            statements += 1;
            if derived == 0 {
                break;
            }
            low = high + 1;
            high += derived;
            rounds += 1;
        }
        let (top, count): (i64, i64) = conn
            .query_row("SELECT max(rowid), count(*) FROM reachable", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(top, count, "rowid range broke: max={top} count={count}");
    }
    conn.execute_batch("COMMIT;").unwrap();
    let fixpoint_ms = fp_started.elapsed().as_millis();

    let fold_started = std::time::Instant::now();
    let derived: i64 = conn
        .query_row("SELECT count(*) FROM reachable", [], |row| row.get(0))
        .unwrap();
    let checksum = fold_reachable(&conn);
    let fold_ms = fold_started.elapsed().as_millis();
    (
        EngineResult {
            derived: derived as u64,
            checksum,
            rounds,
            statements,
        },
        Measures {
            load_ms,
            fixpoint_ms,
            fold_ms,
        },
    )
}

pub fn run_tuned_wave(edges: &[(u32, u32)]) -> (EngineResult, Measures) {
    let started = std::time::Instant::now();
    let conn = open_db();
    create_edge_and_load(&conn, edges);
    let load_ms = started.elapsed().as_millis();

    conn.execute_batch(
        "CREATE TABLE reachable (
            source INTEGER NOT NULL,
            target INTEGER NOT NULL,
            PRIMARY KEY (source, target)
        ) WITHOUT ROWID;
         CREATE TABLE frontier_ping (
            source INTEGER NOT NULL,
            target INTEGER NOT NULL,
            PRIMARY KEY (source, target)
         ) WITHOUT ROWID;
         CREATE TABLE frontier_pong (
            source INTEGER NOT NULL,
            target INTEGER NOT NULL,
            PRIMARY KEY (source, target)
         ) WITHOUT ROWID;",
    )
    .unwrap();

    let fp_started = std::time::Instant::now();
    let mut rounds = 0u64;
    let mut statements = 0u64;
    conn.execute_batch("BEGIN;").unwrap();
    {
        let mut seed = conn
            .prepare("INSERT OR IGNORE INTO frontier_ping (source, target) SELECT source, target FROM edge")
            .unwrap();
        let mut promote_ping = conn
            .prepare("INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM frontier_ping")
            .unwrap();
        let mut promote_pong = conn
            .prepare("INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM frontier_pong")
            .unwrap();
        let mut step_ping_to_pong = conn
            .prepare(
                "INSERT OR IGNORE INTO frontier_pong (source, target)
                 SELECT frontier.source, edge.target
                 FROM frontier_ping frontier JOIN edge ON edge.source = frontier.target
                 WHERE NOT EXISTS (SELECT 1 FROM reachable known
                   WHERE known.source = frontier.source AND known.target = edge.target)",
            )
            .unwrap();
        let mut step_pong_to_ping = conn
            .prepare(
                "INSERT OR IGNORE INTO frontier_ping (source, target)
                 SELECT frontier.source, edge.target
                 FROM frontier_pong frontier JOIN edge ON edge.source = frontier.target
                 WHERE NOT EXISTS (SELECT 1 FROM reachable known
                   WHERE known.source = frontier.source AND known.target = edge.target)",
            )
            .unwrap();
        let mut clear_ping = conn.prepare("DELETE FROM frontier_ping").unwrap();
        let mut clear_pong = conn.prepare("DELETE FROM frontier_pong").unwrap();

        seed.execute([]).unwrap();
        promote_ping.execute([]).unwrap();
        statements += 2;
        let mut use_ping = true;
        loop {
            if use_ping {
                clear_pong.execute([]).unwrap();
                let derived = step_ping_to_pong.execute([]).unwrap();
                statements += 2;
                if derived == 0 {
                    break;
                }
                promote_pong.execute([]).unwrap();
                statements += 1;
            } else {
                clear_ping.execute([]).unwrap();
                let derived = step_pong_to_ping.execute([]).unwrap();
                statements += 2;
                if derived == 0 {
                    break;
                }
                promote_ping.execute([]).unwrap();
                statements += 1;
            }
            rounds += 1;
            use_ping = !use_ping;
        }
    }
    conn.execute_batch("COMMIT;").unwrap();
    let fixpoint_ms = fp_started.elapsed().as_millis();

    let fold_started = std::time::Instant::now();
    let derived: i64 = conn
        .query_row("SELECT count(*) FROM reachable", [], |row| row.get(0))
        .unwrap();
    let checksum = fold_reachable(&conn);
    let fold_ms = fold_started.elapsed().as_millis();
    (
        EngineResult {
            derived: derived as u64,
            checksum,
            rounds,
            statements,
        },
        Measures {
            load_ms,
            fixpoint_ms,
            fold_ms,
        },
    )
}

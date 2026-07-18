//! Storage diet, Direction 5 (2026-07-18, plans/2026-07-18-storage-diet.md,
//! step 2 — index audit): `create_auto_indexes` is now a demand-aware
//! reconcile, not a monotonic-add-only `CREATE INDEX IF NOT EXISTS` loop. On
//! the sprefa root db, `CREATE INDEX IF NOT EXISTS` alone let every `.dl`
//! program ever served leave its join-key indexes behind forever — the
//! measured incident db carried 755 auto-created `idx_<rel>_<col>` indexes
//! (309.8MB, 34% of a 921.9MB db) built up across every program run against
//! that root over its history, not just the currently-discovered set.
//!
//! This file pins:
//!   - a join-key index gets created when a rule needs it
//!     (`join_key_index_is_created_when_a_rule_needs_it`);
//!   - the SAME index gets dropped on the very next tick of a DIFFERENT
//!     program that no longer joins that column — the discriminating case
//!     (`stale_join_key_index_is_pruned_when_no_longer_needed`), proven
//!     fail-pre-fix: with `CREATE INDEX IF NOT EXISTS` alone (pre-fix code),
//!     this index survives forever;
//!   - the index comes back the next time a program needs it again — the
//!     reconcile is non-destructive, not a one-way ratchet
//!     (`pruned_index_is_recreated_when_needed_again`);
//!   - the sweep only ever touches the `idx_<rel>_<col>` naming convention —
//!     a hand-authored index using a different name survives untouched
//!     (`hand_authored_index_survives_the_sweep`);
//!   - a fixture shaped like the measured production join families (shared
//!     join-key text repeated across two source rels) loses real index bytes
//!     when the joining rule drops out of the served program
//!     (`pruning_reclaims_measurable_index_bytes`).

use rusqlite::Connection;
use sprefa_v5::spine::StringId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
// Hermetic: an ad-hoc `dl` run otherwise ingests `~/.config/sprefa/config.toml`
// (the ambient-config friction item in CLAUDE.md) and scans real repos.
const HERMETIC_CONFIG: &str = "/nonexistent/sprefa-hermetic.toml";

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storage_diet_index_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, db: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", db.to_str().unwrap()])
        .env("SPREFA_CONFIG", HERMETIC_CONFIG)
        .current_dir(dir)
        .output()
        .expect("run dl");
    assert!(
        out.status.success(),
        "dl failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn index_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1")
        .unwrap();
    stmt.query_map([table], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

// Two source rels joined on `name` (a Rust struct name shared by two match
// rules over the same files), plus a derived rel that performs the join —
// the classic auto_indexes() shape: `name` occupies position 0 in both
// `symbol` and `other` body atoms of the `joined` rule, so both get an
// `idx_<rel>_name` index.
const SRC: &str = "struct AuthService;\nstruct BillingGateway;\n";

const WITH_JOIN: &str = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel other(name: text, path: file).
other(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel joined(name: text, path1: file, path2: file).
joined(name, path1, path2) <- symbol(name, path1), other(name, path2).
? joined(name, path1, path2).
"#;

// Same two source rels, no join: `name` occupies only one atom position per
// rule body, so auto_indexes() no longer wants idx_symbol_name/idx_other_name.
const WITHOUT_JOIN: &str = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel other(name: text, path: file).
other(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#;

#[test]
fn join_key_index_is_created_when_a_rule_needs_it() {
    let d = sandbox("create");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    let other_idx = index_names(&conn, "rel_other");
    assert!(
        symbol_idx.iter().any(|n| n == "idx_symbol_name"),
        "expected idx_symbol_name, got {symbol_idx:?}"
    );
    assert!(
        other_idx.iter().any(|n| n == "idx_other_name"),
        "expected idx_other_name, got {other_idx:?}"
    );
}

#[test]
fn stale_join_key_index_is_pruned_when_no_longer_needed() {
    let d = sandbox("prune");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        assert!(index_names(&conn, "rel_symbol").iter().any(|n| n == "idx_symbol_name"));
        assert!(index_names(&conn, "rel_other").iter().any(|n| n == "idx_other_name"));
    }

    // A different program served against the SAME db: the join is gone, so
    // neither rel needs its idx_<rel>_name anymore. Pre-fix (CREATE INDEX IF
    // NOT EXISTS with no reconcile), both indexes survive forever — this is
    // the exact accumulation shape that grew the incident db to 755 indexes.
    run(&d, &db, WITHOUT_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    let other_idx = index_names(&conn, "rel_other");
    assert!(
        !symbol_idx.iter().any(|n| n == "idx_symbol_name"),
        "stale index must be pruned once no rule joins on it: {symbol_idx:?}"
    );
    assert!(
        !other_idx.iter().any(|n| n == "idx_other_name"),
        "stale index must be pruned once no rule joins on it: {other_idx:?}"
    );
}

#[test]
fn pruned_index_is_recreated_when_needed_again() {
    let d = sandbox("recreate");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    run(&d, &db, WITHOUT_JOIN); // prunes idx_symbol_name / idx_other_name
    run(&d, &db, WITH_JOIN); // the join comes back

    let conn = Connection::open(&db).unwrap();
    assert!(index_names(&conn, "rel_symbol").iter().any(|n| n == "idx_symbol_name"));
    assert!(index_names(&conn, "rel_other").iter().any(|n| n == "idx_other_name"));
}

#[test]
fn hand_authored_index_survives_the_sweep() {
    let d = sandbox("hand_index");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        // A name that does NOT start with the `idx_` auto-created prefix —
        // the reconcile sweep must never touch it, even though it indexes
        // the same table the sweep manages.
        conn.execute("CREATE INDEX hand_rolled_symbol_path ON rel_symbol(path)", [])
            .unwrap();
    }

    // Switch to the no-join program: prunes idx_symbol_name, must leave the
    // hand-authored index alone.
    run(&d, &db, WITHOUT_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    assert!(
        symbol_idx.iter().any(|n| n == "hand_rolled_symbol_path"),
        "hand-authored index must survive the idx_ sweep: {symbol_idx:?}"
    );
    assert!(!symbol_idx.iter().any(|n| n == "idx_symbol_name"));
}

fn object_bytes(conn: &Connection, name: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = ?1",
        [name],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Receipt test: a fixture where `rel_symbol`/`rel_other` carry enough rows
/// to give their join-key indexes real weight loses those index bytes when
/// the joining rule drops out of the served program. Not a byte-count
/// regression pin (page layout drifts with SQLite/rusqlite versions); it
/// asserts the delta is positive and prints it for the receipt.
#[test]
fn pruning_reclaims_measurable_index_bytes() {
    const ROWS: usize = 4000;
    let d = sandbox("bytes");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN); // creates schema + idx_symbol_name/idx_other_name

    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        {
            let mut str_stmt = conn
                .prepare("INSERT OR IGNORE INTO _strings (id, content) VALUES (?1, ?2)")
                .unwrap();
            let mut symbol_stmt = conn
                .prepare("INSERT OR IGNORE INTO rel_symbol (name, path, __src) VALUES (?1, ?2, '')")
                .unwrap();
            let mut other_stmt = conn
                .prepare("INSERT OR IGNORE INTO rel_other (name, path, __src) VALUES (?1, ?2, '')")
                .unwrap();
            for i in 0..ROWS {
                let name = format!("Struct{i:04}");
                let path = format!("src/module_{i:04}/file_{i:04}.rs");
                let name_id = StringId::of(&name).sqlite();
                let path_id = StringId::of(&path).sqlite();
                str_stmt.execute(rusqlite::params![name_id, name]).unwrap();
                str_stmt.execute(rusqlite::params![path_id, path]).unwrap();
                symbol_stmt.execute(rusqlite::params![name_id, path_id]).unwrap();
                other_stmt.execute(rusqlite::params![name_id, path_id]).unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        conn.execute_batch("VACUUM").unwrap();
    }

    let (before_symbol_idx, before_other_idx) = {
        let conn = Connection::open(&db).unwrap();
        (object_bytes(&conn, "idx_symbol_name"), object_bytes(&conn, "idx_other_name"))
    };
    assert!(before_symbol_idx > 0, "fixture's join-key index must carry real bytes pre-prune");
    assert!(before_other_idx > 0, "fixture's join-key index must carry real bytes pre-prune");

    run(&d, &db, WITHOUT_JOIN); // the join drops out; both indexes should prune

    let (after_symbol_idx, after_other_idx, db_before, db_after) = {
        let conn = Connection::open(&db).unwrap();
        let before_bytes: i64 = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute_batch("VACUUM").unwrap();
        let after_bytes: i64 = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |r| r.get(0),
            )
            .unwrap();
        (object_bytes(&conn, "idx_symbol_name"), object_bytes(&conn, "idx_other_name"), before_bytes, after_bytes)
    };
    assert_eq!(after_symbol_idx, 0, "idx_symbol_name object must be gone post-VACUUM");
    assert_eq!(after_other_idx, 0, "idx_other_name object must be gone post-VACUUM");

    let index_delta = (before_symbol_idx + before_other_idx) - (after_symbol_idx + after_other_idx);
    let db_delta = db_before - db_after;
    eprintln!(
        "[storage-diet receipt] idx_symbol_name+idx_other_name before={} after=0 delta={} bytes; \
         db before={db_before} after={db_after} delta={db_delta} bytes; rows={ROWS}",
        before_symbol_idx + before_other_idx,
        index_delta,
    );
    assert!(index_delta > 0, "pruning must reclaim index bytes");
    assert!(db_delta > 0, "pruning must reclaim measurable db bytes on VACUUM");
}

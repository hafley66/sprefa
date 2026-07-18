//! Storage diet, Direction 3a+3b (2026-07-18, plans/2026-07-18-storage-diet.md):
//! `_strings.norm` and its index are gone. The column had exactly one reader
//! (the `string(id,text,norm)` rel projection at src/engine/extract/mod.rs);
//! the dl `norm()` builtin was always the query-time `sprf_norm` scalar
//! (src/db.rs), never a read of the stored column. This file pins:
//!
//!   - a fresh db carries no `_strings.norm` column and no `_strings_norm_idx`
//!     index (`fresh_db_has_no_norm_column_or_index`);
//!   - the `string` rel's third column is byte-identical, for EVERY interned
//!     row, to what the `norm()` builtin computes over the same text
//!     (`string_rel_third_column_matches_norm_builtin_for_every_row`);
//!   - an existing OLD-schema db (column + index still present, as shipped
//!     before this arc) is migrated in place on open: column and index gone,
//!     the db still answers queries (`old_schema_db_is_migrated_on_open`);
//!   - the migration is idempotent — a second open of an already-migrated db
//!     is a schema no-op (`migration_is_idempotent_on_second_open`);
//!   - the migration reclaims real bytes on a fixture db shaped like the
//!     measured production corpus (`migration_reclaims_measurable_bytes`).

use rusqlite::Connection;
use sprefa_v5::spine::{normalize, StringId};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
// Underscores are not valid Rust identifier punctuation to strip, so lean on
// digits + case to prove the fold is doing real work (ASCII-alnum keep,
// lowercase, drop the rest) without inventing punctuation `struct` can't parse.
const SRC: &str = "struct AuthService2;\nstruct BillingGateway9;\n";
// Hermetic: an ad-hoc `dl` run otherwise ingests `~/.config/sprefa/config.toml`
// (the ambient-config friction item in CLAUDE.md) and scans real repos.
const HERMETIC_CONFIG: &str = "/nonexistent/sprefa-hermetic.toml";

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storage_diet_norm_{tag}"));
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

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1)).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap()
}

fn index_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1")
        .unwrap();
    stmt.query_map([table], |r| r.get::<_, String>(0)).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap()
}

/// Data rows from a `?` query's tab-separated body, skipping the `? rel =>
/// ...` header (starts with `?`) and the `  (N rows)` footer (leading
/// whitespace).
fn data_rows(out: &str) -> Vec<Vec<&str>> {
    out.lines()
        .filter(|line| !line.starts_with('?'))
        .filter(|line| line.trim_start() == *line)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split('\t').collect())
        .collect()
}

#[test]
fn fresh_db_has_no_norm_column_or_index() {
    let d = sandbox("fresh_schema");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    run(&d, &d.join("db"), r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#);

    let conn = Connection::open(d.join("db")).unwrap();
    assert_eq!(
        columns(&conn, "_strings"),
        vec!["id".to_string(), "content".to_string()],
        "no stored norm column on a freshly created db"
    );
    let idx = index_names(&conn, "_strings");
    assert!(
        !idx.iter().any(|n| n == "_strings_norm_idx"),
        "no norm index on a freshly created db: {idx:?}"
    );
}

#[test]
fn string_rel_third_column_matches_norm_builtin_for_every_row() {
    let d = sandbox("string_rel_norm");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel located(t: text, norm_val: text, want: text).
located(t, norm_val, want) <- string(_, t, norm_val), want = norm(t).
? located(t, norm_val, want).
"#;
    let out = run(&d, &d.join("db"), prog);

    let mut seen_auth = false;
    let mut seen_billing = false;
    let mut rows = 0;
    for cols in data_rows(&out) {
        assert_eq!(cols.len(), 3, "expected 3 tab-separated columns: {cols:?}\nfull output:\n{out}");
        let (text, norm_val, want) = (cols[0], cols[1], cols[2]);
        assert_eq!(
            norm_val, want,
            "string rel's stored-shape norm column diverged from the norm() builtin for {text:?}"
        );
        if text == "AuthService2" {
            assert_eq!(norm_val, "authservice2");
            seen_auth = true;
        }
        if text == "BillingGateway9" {
            assert_eq!(norm_val, "billinggateway9");
            seen_billing = true;
        }
        rows += 1;
    }
    assert!(rows > 0, "expected at least one interned string row:\n{out}");
    assert!(seen_auth, "AuthService2 row missing:\n{out}");
    assert!(seen_billing, "BillingGateway9 row missing:\n{out}");
}

/// Hand-build the pre-migration `_strings` shape (column + index) exactly as
/// it shipped before this arc, insert the zero sentinel the engine expects,
/// and return the db path.
fn old_schema_db(dir: &Path) -> PathBuf {
    let db_path = dir.join("db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL, norm TEXT NOT NULL);
         CREATE INDEX _strings_norm_idx ON _strings(norm);
         INSERT INTO _strings (id, content, norm) VALUES (0, '', '');",
    ).unwrap();
    drop(conn);
    db_path
}

#[test]
fn old_schema_db_is_migrated_on_open() {
    let d = sandbox("migrate_open");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db_path = old_schema_db(&d);

    {
        let conn = Connection::open(&db_path).unwrap();
        assert!(columns(&conn, "_strings").iter().any(|c| c == "norm"), "fixture setup sanity");
        assert!(index_names(&conn, "_strings").iter().any(|n| n == "_strings_norm_idx"));
    }

    let out = run(&d, &db_path, r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#);
    assert!(out.contains("AuthService2"), "db still answers queries post-migration:\n{out}");

    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(
        columns(&conn, "_strings"),
        vec!["id".to_string(), "content".to_string()],
        "norm column dropped by the on-open migration"
    );
    assert!(
        !index_names(&conn, "_strings").iter().any(|n| n == "_strings_norm_idx"),
        "norm index dropped by the on-open migration"
    );
}

#[test]
fn migration_is_idempotent_on_second_open() {
    let d = sandbox("migrate_idempotent");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db_path = old_schema_db(&d);
    let prog = r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#;
    run(&d, &db_path, prog); // first open: migrates
    let out2 = run(&d, &db_path, prog); // second open: must be a schema no-op, not an error
    assert!(out2.contains("AuthService2"), "second run still answers queries:\n{out2}");

    let conn = Connection::open(&db_path).unwrap();
    assert_eq!(columns(&conn, "_strings"), vec!["id".to_string(), "content".to_string()]);
    assert!(!index_names(&conn, "_strings").iter().any(|n| n == "_strings_norm_idx"));
}

fn object_bytes(conn: &Connection, name: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = ?1",
        [name],
        |r| r.get(0),
    ).unwrap_or(0)
}

/// Receipt test: a fixture `_strings` table shaped like the measured
/// production corpus (plan §1 — avg content ~43 chars, avg norm ~34 chars,
/// coordinate-composite-shaped one-use strings) loses real bytes when
/// migrated. Not a byte-count regression pin (page layout drifts with
/// SQLite/rusqlite versions); it asserts the delta is positive and prints it
/// for the receipt.
#[test]
fn migration_reclaims_measurable_bytes() {
    const ROWS: usize = 4000;
    let d = sandbox("migrate_bytes");
    fs::write(d.join("src/a.rs"), SRC).unwrap();
    let db_path = old_schema_db(&d);

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        {
            let mut stmt = conn
                .prepare("INSERT INTO _strings (id, content, norm) VALUES (?1, ?2, ?3)")
                .unwrap();
            for i in 0..ROWS {
                // Coordinate-composite shape from the plan's measurement:
                // "src/module_XXXX/file_XXXX.rs:LINE:COL:kind_something".
                let content = format!(
                    "src/module_{i:04}/file_{i:04}.rs:{line}:{col}:kind_something",
                    line = i * 3 + 1,
                    col = i % 80,
                );
                let norm = normalize(&content);
                let id = StringId::of(&content).sqlite();
                stmt.execute(rusqlite::params![id, content, norm]).unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        conn.execute_batch("VACUUM").unwrap();
    }

    let (before_table, before_index) = {
        let conn = Connection::open(&db_path).unwrap();
        (object_bytes(&conn, "_strings"), object_bytes(&conn, "_strings_norm_idx"))
    };
    assert!(before_index > 0, "fixture's norm index must carry real bytes pre-migration");

    run(&d, &db_path, r#"
rel symbol(name: text, path: file).
symbol(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(name, path).
"#);

    let after_table = {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("VACUUM").unwrap();
        let bytes = object_bytes(&conn, "_strings");
        assert_eq!(object_bytes(&conn, "_strings_norm_idx"), 0, "index object must be gone post-VACUUM");
        bytes
    };

    let delta = (before_table + before_index) - after_table;
    eprintln!(
        "[storage-diet receipt] _strings+idx before={before_table_plus_index} \
         (table={before_table}, idx={before_index}), _strings after={after_table}, delta={delta} bytes, \
         rows={ROWS}",
        before_table_plus_index = before_table + before_index,
    );
    assert!(
        delta > 0,
        "migration must reclaim bytes: before(table+idx)={} after(table)={}",
        before_table + before_index,
        after_table
    );
}

//! Storage diet, Direction 4a (2026-07-18, plans/2026-07-18-storage-diet.md,
//! step 4a — autoindex elimination on pure junction/set rels): SQLite backs a
//! plain rowid table's full-row `PRIMARY KEY` with a UNIQUE autoindex that
//! duplicates the entire row (measured: rel_flow_edge 8.6MB table + 8.6MB
//! autoindex, rel_df_edge_src_kind 8.3+8.3MB). `Engine::declare`
//! (src/engine/declare.rs) now emits `WITHOUT ROWID` for any rel decl
//! `wants_without_rowid` classifies as a pure junction: an explicit
//! `pk_never_null` vouch, no `key(...)` narrowing (full row = PK, Soufflé set
//! semantics), every column stores as SQLite INTEGER, and 2..=4 columns (the
//! plan's own scope, guarding against the secondary-index locator-growth risk
//! a wide entity/occurrence table like df_node would hit).
//!
//! `pk_never_null` exists because of an incident this arc caught in its own
//! it-suite run: the FIRST version of this classifier judged by shape alone
//! (no `key()`, all-INTEGER, 2..=4 cols) and silently broke
//! tests/it/named_args.rs `named_args_in_a_rule_head_resolve` —
//! `WITHOUT ROWID` requires every PK column NOT NULL, but a plain rowid
//! table's composite `PRIMARY KEY` tolerates NULL (SQLite treats it as
//! always-distinct there), and the `.dl` surface's named-arg partial-head
//! padding (`person(name: "z").` intentionally leaves `age` NULL) can put a
//! NULL in any column of any parsed rel. `INSERT OR IGNORE` silently dropped
//! the NULL-padded row under the shape-only classifier. `pk_never_null`
//! closes that: it defaults `false` for every `.dl`-parsed decl and is only
//! set `true` at specific Rust-authored decls (src/engine/decls.rs
//! dataflow_rel_decls, src/rels/scip.rs ScipKind::decls) whose insert path is
//! read and confirmed to push a fixed-arity, non-`Option` literal for every
//! row, every time. See docs/failure-modes.md for the incident entry.
//!
//! This file pins the end-to-end DDL wiring through the real `dl` binary
//! (the classifier's boundary cases — raw text column, column-count edges,
//! the `pk_never_null` gate itself — live as pure-fn unit tests beside
//! `wants_without_rowid` in src/engine/declare.rs, cheaper there since the
//! `.dl` surface cannot express a raw-TEXT-column or `pk_never_null: true`
//! rel):
//!   - a vouched, Rust-authored, narrow all-integer junction (`df_edge`,
//!     driven through a real dataflow extraction, not a synthetic fixture)
//!     gets `WITHOUT ROWID` and carries NO `sqlite_autoindex_*` object
//!     (`vouched_builtin_junction_gets_without_rowid_and_no_autoindex`);
//!   - an ordinary `.dl`-declared rel of the IDENTICAL shape (2-col,
//!     all-integer, no `key()`) — the exact shape that regressed — stays a
//!     rowid table, because `.dl`-parsed decls never carry the
//!     `pk_never_null` vouch
//!     (`unvouched_dl_declared_rel_of_the_same_shape_stays_a_rowid_table`);
//!   - the identical column shape with a `key(...)` qualifier — no longer a
//!     pure junction, the full row isn't the PK — also stays a rowid table
//!     (`key_narrowed_rel_stays_a_rowid_table`);
//!   - a 5-column all-integer no-key rel — outside the plan's 2..=4 scope —
//!     also stays a rowid table (`wide_rel_stays_a_rowid_table`);
//!   - a byte receipt: an identical row set stored WITHOUT ROWID vs. the
//!     pre-4a rowid+autoindex shape measurably shrinks
//!     (`without_rowid_reclaims_measurable_bytes_vs_pre_4a_shape`), proven
//!     fail-pre-fix by literally building both DDL shapes with rusqlite and
//!     comparing — the pre-4a shape is exactly what `declare` emitted before
//!     this change.

use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
// Hermetic: an ad-hoc `dl` run otherwise ingests `~/.config/sprefa/config.toml`
// (the ambient-config friction item in CLAUDE.md) and scans real repos.
const HERMETIC_CONFIG: &str = "/nonexistent/sprefa-hermetic.toml";

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storage_diet_without_rowid_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
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

fn table_sql(conn: &Connection, table: &str) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("no such table {table}: {e}"))
}

fn autoindex_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1 \
             AND name LIKE 'sqlite_autoindex_%'",
        )
        .unwrap();
    stmt.query_map([table], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// A real dataflow lift (syn over a tiny Rust source, same fixture shape as
/// tests/it/dataflow.rs) forces `df_edge` — a Rust-authored, `pk_never_null:
/// true` dataflow decl (src/engine/decls.rs) — to be created AND populated
/// with real rows, not just declared empty.
#[test]
fn vouched_builtin_junction_gets_without_rowid_and_no_autoindex() {
    let d = sandbox("df_edge");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn greet(g: &str) -> String { String::new() }\n\
         fn f(name: &str) {\n    \
             let s = greet(name);\n\
         }\n",
    )
    .unwrap();
    let db = d.join("db");
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "? df_edge(from, to).\n",
    );
    let out = run(&d, &db, prog);
    assert!(out.contains("row"), "expected df_edge to carry at least one row:\n{out}");

    let conn = Connection::open(&db).unwrap();
    let sql = table_sql(&conn, "rel_df_edge");
    assert!(
        sql.to_uppercase().ends_with("WITHOUT ROWID"),
        "df_edge is a vouched Rust-authored 2-col all-integer no-key rel, expected WITHOUT ROWID: {sql}"
    );
    assert!(
        autoindex_names(&conn, "rel_df_edge").is_empty(),
        "WITHOUT ROWID table must carry no duplicate full-row PK autoindex"
    );
    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM rel_df_edge", [], |r| r.get(0)).unwrap();
    assert!(rows > 0, "fixture must actually populate df_edge, not just declare it");
}

// The IDENTICAL column shape as `df_edge` (2 columns, all-integer via
// interning, no key()) but declared through the `.dl` parser — the shape
// that regressed tests/it/named_args.rs `named_args_in_a_rule_head_resolve`
// before `pk_never_null` existed. Named-arg partial-head padding can leave
// either column NULL for ANY `.dl`-parsed rel of this shape (not just
// `person`), so this must NEVER get WITHOUT ROWID regardless of how closely
// it resembles a vouched built-in.
const DL_DECLARED_SAME_SHAPE: &str = r#"
rel edge_ab(a: text, b: text).
edge_ab("n0", "n1").
edge_ab("n1", "n2").
? edge_ab(a, b).
"#;

#[test]
fn unvouched_dl_declared_rel_of_the_same_shape_stays_a_rowid_table() {
    let d = sandbox("unvouched");
    let db = d.join("db");
    run(&d, &db, DL_DECLARED_SAME_SHAPE);

    let conn = Connection::open(&db).unwrap();
    let sql = table_sql(&conn, "rel_edge_ab");
    assert!(
        !sql.to_uppercase().contains("WITHOUT ROWID"),
        "a .dl-parsed rel never carries the pk_never_null vouch, even when its shape matches a vouched built-in: {sql}"
    );
    assert!(
        !autoindex_names(&conn, "rel_edge_ab").is_empty(),
        "an un-vouched rowid table still carries its full-row PK autoindex"
    );
}

// Three columns, `key(a, b)` narrows the PK to a strict (composite) subset:
// the full row is no longer the uniqueness contract, so this is not a pure
// junction per storage-key-audit decision 2. A *single*-column INTEGER PK
// would make SQLite alias the column to rowid (no autoindex either way, but
// for an unrelated reason predating this change) — key(a, b) keeps this a
// genuine multi-column PK so the autoindex assertion below actually
// discriminates the rowid-table shape from WITHOUT ROWID.
const KEYED: &str = r#"
rel edge_abc(a: text, b: text, c: text) key(a, b).
edge_abc("n0", "n1", "n2").
edge_abc("n1", "n2", "n3").
? edge_abc(a, b, c).
"#;

#[test]
fn key_narrowed_rel_stays_a_rowid_table() {
    let d = sandbox("keyed");
    let db = d.join("db");
    run(&d, &db, KEYED);

    let conn = Connection::open(&db).unwrap();
    let sql = table_sql(&conn, "rel_edge_abc");
    assert!(
        !sql.to_uppercase().contains("WITHOUT ROWID"),
        "a key(...)-narrowed rel must stay a rowid table: {sql}"
    );
    assert!(
        !autoindex_names(&conn, "rel_edge_abc").is_empty(),
        "a rowid table with a composite PRIMARY KEY still carries its autoindex"
    );
}

// Five all-integer columns, no key(): outside the plan's 2..=4 scope. Wide
// entity/occurrence tables (df_node at 6 cols + 6 join-key indexes,
// scip_occurrence at 8) ride Direction 1/4b (narrow the PK first) instead —
// applying WITHOUT ROWID here would grow every secondary index's locator
// from an 8-byte rowid to the full multi-column PK.
const WIDE: &str = r#"
rel wide_edge(a: text, b: text, c: text, d: text, e: text).
wide_edge("n0", "n1", "n2", "n3", "n4").
? wide_edge(a, b, c, d, e).
"#;

#[test]
fn wide_rel_stays_a_rowid_table() {
    let d = sandbox("wide");
    let db = d.join("db");
    run(&d, &db, WIDE);

    let conn = Connection::open(&db).unwrap();
    let sql = table_sql(&conn, "rel_wide_edge");
    assert!(
        !sql.to_uppercase().contains("WITHOUT ROWID"),
        "a 5-column rel is outside step 4a's 2..=4 column scope: {sql}"
    );
    assert!(!autoindex_names(&conn, "rel_wide_edge").is_empty());
}

fn total_db_bytes(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

/// Receipt test: build the IDENTICAL logical rel (2 INTEGER columns, full row
/// = PK, `ROWS` distinct tuples) two ways with raw SQL — the post-4a shape
/// (`WITHOUT ROWID`, what a vouched decl like `df_edge` now gets) and the
/// pre-4a shape (a plain rowid table, what EVERY rel — vouched or not — got
/// before this change: `CREATE TABLE ... (a INTEGER, b INTEGER, __src TEXT
/// DEFAULT '', PRIMARY KEY (a, b))` with no `WITHOUT ROWID` suffix).
/// Fail-pre-fix by construction: the "pre" branch here IS the old code's
/// output, so this test would show zero delta (or a negative one) if
/// `wants_without_rowid`/the DDL suffix were reverted. Not a byte-count
/// regression pin (page layout drifts with SQLite/rusqlite versions); it
/// asserts the delta is positive and prints it for the receipt.
#[test]
fn without_rowid_reclaims_measurable_bytes_vs_pre_4a_shape() {
    const ROWS: i64 = 4000;
    let d = sandbox("bytes");

    let post_db = d.join("post.db");
    let pre_db = d.join("pre.db");

    let post = Connection::open(&post_db).unwrap();
    post.execute_batch(
        "CREATE TABLE rel_edge_ab (a INTEGER, b INTEGER, __src TEXT DEFAULT '', \
         PRIMARY KEY (a, b)) WITHOUT ROWID;",
    )
    .unwrap();

    let pre = Connection::open(&pre_db).unwrap();
    pre.execute_batch(
        "CREATE TABLE rel_edge_ab (a INTEGER, b INTEGER, __src TEXT DEFAULT '', \
         PRIMARY KEY (a, b));",
    )
    .unwrap();

    for conn in [&post, &pre] {
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        {
            let mut stmt = conn
                .prepare("INSERT OR IGNORE INTO rel_edge_ab (a, b, __src) VALUES (?1, ?2, '')")
                .unwrap();
            for i in 0..ROWS {
                stmt.execute(rusqlite::params![i, i + 1]).unwrap();
            }
        }
        conn.execute_batch("COMMIT; VACUUM;").unwrap();
    }

    let post_bytes = total_db_bytes(&post);
    let pre_bytes = total_db_bytes(&pre);
    let delta = pre_bytes - post_bytes;
    eprintln!(
        "[storage-diet step 4a receipt] rows={ROWS} pre-4a (rowid+autoindex)={pre_bytes} bytes \
         post-4a (WITHOUT ROWID)={post_bytes} bytes delta={delta} bytes ({:.1}%)",
        100.0 * delta as f64 / pre_bytes as f64
    );
    assert!(
        autoindex_names(&pre, "rel_edge_ab").len() == 1,
        "pre-4a control must carry exactly the one full-row PK autoindex this step removes"
    );
    assert!(autoindex_names(&post, "rel_edge_ab").is_empty());
    assert!(delta > 0, "WITHOUT ROWID must reclaim measurable bytes over the pre-4a rowid+autoindex shape");
}

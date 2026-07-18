//! Lock-free read path for the daemon's row/query RPCs.
//!
//! Every per-root RPC used to funnel through `daemon::lock_eng` — the same
//! `Mutex<Engine>` a tick holds for its whole synchronous run-to-completion body
//! (seconds under slow rules). A client asking for rows then waited behind the
//! tick, so read latency tracked tick duration.
//!
//! The row/query READS, though, only need committed SQLite state: the served db
//! is kept in WAL mode (`db::open`), and the engine renders family flips + tick
//! writes in single transactions, so the on-disk db between commits is a
//! consistent snapshot. This module answers those reads from a fresh READ-ONLY
//! connection (`db::open_read_only`) that never blocks on the writer and never
//! takes the engine mutex.
//!
//! Lowering a `?` query needs the relation shapes (`Rels`), which live only in
//! the `Engine` behind that mutex. Rather than lock it, `ServedRoot` keeps a
//! [`ReadView`] snapshot of the shape state under a cheap `RwLock`, refreshed
//! whenever the program (re)loads (a data tick never changes it). A read RPC
//! clones the `Arc<ReadView>` and answers from it plus a read-only connection —
//! no `lock_eng`, no `prog` lock.
//!
//! What STAYS on the engine lock (see `dispatch_root`): `diag` (also reads the
//! process-resident `shape_diags`/`extraction_drops`, not in SQLite), `what` /
//! `summary` / `definition` / `hover` (read the db but through `&Engine`; not
//! ported here), aggregate `?` queries and `query`/`query_sql` on the
//! in-memory-db fallback (an aggregate interns a fresh derived string at query
//! time that a read-only connection cannot persist), and every mutating method.
//! `q` / `eval` / `load` already run against a throwaway in-memory engine and
//! never touched `lock_eng`.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{json, Value};

use crate::db::ReadDb;

use crate::ast::{AggFn, Item, Program, Query, Rels, Term};
use crate::lower::{lower_query, txt_tbl};
use crate::rpc::{INTERNAL_ERROR, INVALID_PARAMS};

/// Immutable snapshot of the shape state a read RPC needs to answer WITHOUT the
/// engine mutex: the declared relation metadata (for lowering `?` queries and
/// building column lists / the `schema` response) and the program's `?`
/// queries, plus the served db file path. A data tick never changes any of
/// this; only a program (re)load does, which is where `ServedRoot` refreshes it.
pub struct ReadView {
    /// Clone of the engine's `rels` as of the last program load.
    pub rels: Rels,
    /// The program's `?` query items as of the last program load.
    pub queries: Vec<Query>,
    /// The served root's on-disk db file. `None` only for a hypothetical
    /// in-memory served root (none exist today); a `None` sends every read to
    /// the engine-lock fallback.
    pub db_path: Option<PathBuf>,
}

impl ReadView {
    /// Build from the engine's declared `rels`, the program's `?` queries, and
    /// the served db path. Called under the `prog`+`eng` locks the tick already
    /// holds, so it is a straight clone with no extra locking.
    pub fn snapshot(rels: &Rels, prog: &Program, db_path: Option<PathBuf>) -> ReadView {
        let queries = prog
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Query(q) => Some(q.clone()),
                _ => None,
            })
            .collect();
        ReadView { rels: rels.clone(), queries, db_path }
    }

    /// The served db path as a `&str`, or `None` when this view cannot be served
    /// lock-free (no on-disk db).
    fn db_path_str(&self) -> Option<&str> {
        self.db_path.as_deref().and_then(|p| p.to_str())
    }

    /// True if any `?` query aggregates. An aggregate lowers to
    /// `sprf_sym_intern(json_group_...())` and then DECODES that id through
    /// `_strings`; the id is for a string interned at query time, which a
    /// read-only connection cannot persist, so the whole `query` RPC falls back
    /// to the engine lock in that case.
    fn any_aggregate_query(&self) -> bool {
        self.queries.iter().any(|q| {
            q.head.terms.iter().any(|t| {
                matches!(t, Term::Call { name, .. } if AggFn::parse(name).is_some())
            })
        })
    }
}

/// `None` = this RPC cannot be served lock-free; the caller falls back to the
/// engine-locked path. `Some(Ok)` / `Some(Err)` = served (map to
/// `Response::ok` / `Response::err`). The error carries the same `(code, msg)`
/// the locked path would have returned.
pub type Served = Option<Result<Value, (i64, String)>>;

/// The `query` RPC: run every `?` query in the program off a read-only
/// connection. Returns `None` (fall back to the engine lock) when the program
/// has no on-disk db or contains an aggregate query. Result shape is byte-for-
/// byte the locked path's (`run_queries_capture` -> `{"results": [...]}`), minus
/// the `_query_log` append the locked path does (a read-only connection cannot
/// write; the daemon read log intentionally does not cover the fast path).
pub fn query(view: &ReadView) -> Served {
    let db_path = view.db_path_str()?;
    if view.any_aggregate_query() {
        return None;
    }
    Some(run_query(view, db_path))
}

fn run_query(view: &ReadView, db_path: &str) -> Result<Value, (i64, String)> {
    let conn = ReadDb::open(db_path)
        .map_err(|e| (INTERNAL_ERROR, format!("open read-only db: {e}")))?;
    let mut results: Vec<Value> = Vec::with_capacity(view.queries.len());
    for q in &view.queries {
        let (sql, columns) =
            lower_query(q, &view.rels).map_err(|e| (INTERNAL_ERROR, format!("{e}")))?;
        let rows = json_rows(&conn, &sql, &[]).map_err(|e| (INTERNAL_ERROR, format!("{e}")))?;
        results.push(json!({ "rel": q.head.rel, "columns": columns, "rows": rows }));
    }
    Ok(json!({ "results": results }))
}

/// The `query_rel` RPC: read one relation's decoded rows (`rel_<name>_txt`
/// view) off a read-only connection. Returns `None` to fall back when there is
/// no on-disk db; `Some(Err(INVALID_PARAMS))` for an unknown relation (matching
/// the locked path). Result shape matches `eng.rel_rows` -> `{"columns", "rows"}`.
pub fn query_rel(view: &ReadView, rel: &str) -> Served {
    let db_path = view.db_path_str()?;
    let Some(meta) = view.rels.get(rel) else {
        return Some(Err((INVALID_PARAMS, format!("unknown relation {rel:?}"))));
    };
    let cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
    let ncols = cols.len();
    Some((|| {
        let conn = ReadDb::open(db_path)
            .map_err(|e| (INTERNAL_ERROR, format!("open read-only db: {e}")))?;
        let rows = string_rows(&conn, &txt_tbl(rel), ncols);
        Ok(json!({ "columns": cols, "rows": rows }))
    })())
}

/// The `query_sql` RPC: run client SQL off a read-only connection. Returns
/// `None` to fall back when there is no on-disk db. Same param binding + row
/// shaping as `eng.query_sql` -> `{"rows": [...]}`. As with `query`, the
/// `_query_log` append the locked path does is skipped (read-only connection).
pub fn query_sql(view: &ReadView, sql: &str, params: &[Value]) -> Served {
    let db_path = view.db_path_str()?;
    Some((|| {
        let conn = ReadDb::open(db_path)
            .map_err(|e| (INTERNAL_ERROR, format!("open read-only db: {e}")))?;
        let rows = json_rows(&conn, sql, params).map_err(|e| (INTERNAL_ERROR, format!("{e}")))?;
        Ok(json!({ "rows": rows }))
    })())
}

/// The `schema` RPC: relation names + column shapes, drawn from the snapshot
/// `rels` (no connection needed). Byte-for-byte the locked path's output. Always
/// served — the snapshot is the authority on shapes and is refreshed on every
/// program load.
pub fn schema(view: &ReadView) -> Value {
    let builtin = crate::engine::builtin_rel_names();
    let mut relations: Vec<Value> = Vec::new();
    for (name, meta) in view.rels.iter() {
        let cols: Vec<Value> = meta
            .cols
            .iter()
            .map(|c| json!({ "name": c.name, "ty": format!("{:?}", c.ty) }))
            .collect();
        let mut rel = json!({ "name": name, "columns": cols });
        if builtin.contains(name) {
            rel["builtin"] = Value::Bool(true);
        }
        relations.push(rel);
    }
    let extra: &[(&str, &[(&str, &str)])] = &[
        ("_file", &[("repo", "text"), ("path", "text"), ("rev", "text"), ("hash", "text"), ("mtime", "int"), ("size", "int")]),
        ("_files", &[("id", "int"), ("content_hash", "text"), ("path", "text"), ("size", "int")]),
        ("_where_bytes", &[("id", "int"), ("repo", "text"), ("path", "text"), ("rev", "text"), ("byte", "int"), ("line", "int"), ("col", "int")]),
        ("_program", &[("path", "text"), ("hash", "text"), ("mtime", "int"), ("loaded_at", "int")]),
        ("_repo", &[("slug", "text"), ("root", "text"), ("url", "text"), ("registered_at", "int")]),
        ("_ref", &[("repo", "text"), ("name", "text"), ("oid", "text"), ("observed_at", "int")]),
        ("_rev_log", &[("id", "int"), ("repo", "text"), ("name", "text"), ("old", "text"), ("new", "text"), ("at", "int")]),
    ];
    for (name, cols) in extra {
        let cols_json: Vec<Value> =
            cols.iter().map(|(n, t)| json!({ "name": n, "ty": t })).collect();
        relations.push(json!({ "name": name, "columns": cols_json, "builtin": true }));
    }
    json!({ "relations": relations })
}

// ---------- shared row shaping (mirrors the engine's private helpers) ----------

/// Run `sql` and shape rows as JSON values, matching `engine::query.rs`'s
/// `sqlite_to_json` (text -> string, int/real -> number, null/blob -> null for
/// the lowered-query path). Params bind like `engine::rpc.rs`'s `query_sql`.
fn json_rows(conn: &ReadDb, sql: &str, params: &[Value]) -> Result<Vec<Vec<Value>>> {
    let sqlval_params: Vec<crate::db::SqlVal> = params.iter().map(crate::db::SqlVal::from_json).collect();
    let raw_rows = conn.query_values("_query_sql", sql, &sqlval_params)?;
    Ok(raw_rows
        .iter()
        .map(|row| row.iter().map(sqlval_to_json_rpc).collect())
        .collect())
}

/// One SQLite value -> JSON. Matches `engine::rpc.rs::query_sql`'s cell mapping
/// (blob reports its byte count, null elsewhere).
fn sqlval_to_json_rpc(v: &crate::db::SqlVal) -> Value {
    match v {
        crate::db::SqlVal::Text(s) => Value::String(s.clone()),
        crate::db::SqlVal::Int(n) => Value::from(*n),
        crate::db::SqlVal::Real(f) => Value::from(*f),
        crate::db::SqlVal::Blob(b) => Value::String(format!("<blob {}B>", b.len())),
        crate::db::SqlVal::Null => Value::Null,
    }
}

/// Read `SELECT * FROM <txt_view>` as positional String rows, matching
/// `engine::rpc.rs::rel_rows` (every cell stringified, row dropped on error).
fn string_rows(conn: &ReadDb, txt_view: &str, _ncols: usize) -> Vec<Vec<String>> {
    let raw_rows = match conn.query_values(txt_view, &format!("SELECT * FROM {txt_view}"), &[]) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };
    raw_rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.to_lossy_string()).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// A unique temp dir removed on drop (no `tempfile` dev-dep in this crate).
    struct TmpDir(std::path::PathBuf);
    impl TmpDir {
        fn new() -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("dl_daemon_read_{}_{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TmpDir(dir)
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Build an on-disk engine over a small program, tick it once, and return
    /// (engine, db_path, temp-dir-guard, program). The engine is the WRITER; the
    /// read path opens its own read-only connection on `db_path`.
    fn fixture(program: &str) -> (Engine, String, TmpDir, Program) {
        let dir = TmpDir::new();
        let db_path = dir.0.join("db.sqlite");
        let db_str = db_path.to_string_lossy().into_owned();
        let toks = crate::lex::lex(program).unwrap();
        let mut prog = crate::parse::parse(toks).unwrap();
        let diags = crate::typecheck::check_and_normalize(&mut prog, "<test>");
        assert!(
            !diags.iter().any(|d| d.severity == crate::ast::Severity::Error),
            "fixture program has type errors: {diags:?}"
        );
        let conn = crate::db::open(Some(&db_str)).unwrap();
        let mut eng = Engine::new(conn, dir.0.clone());
        eng.tick(&prog, true).unwrap();
        (eng, db_str, dir, prog)
    }

    const EDGE: &str = "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\nedge(\"b\", \"c\").\nedge(\"c\", \"d\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n";

    fn view_of(eng: &Engine, prog: &Program, db_str: &str) -> ReadView {
        ReadView::snapshot(&eng.rels, prog, Some(PathBuf::from(db_str)))
    }

    /// (a) The read-only path returns the SAME rows as the engine-locked path.
    #[test]
    fn read_path_matches_locked_path() {
        let (eng, db_str, _dir, prog) = fixture(EDGE);
        let view = view_of(&eng, &prog, &db_str);

        // query
        let locked = json!({"results": eng.run_queries_capture(&prog).unwrap().iter()
            .map(|r| json!({"rel": r.rel, "columns": r.columns, "rows": r.rows}))
            .collect::<Vec<_>>()});
        let read = super::query(&view).expect("served").expect("ok");
        assert_eq!(read, locked, "query read path == locked path");
        // The reach closure over the 4-edge line: 3+2+1 = 6 pairs.
        assert_eq!(read["results"][0]["rows"].as_array().unwrap().len(), 6);

        // query_rel
        let cols = 2;
        let locked_rows = eng.rel_rows("edge", cols);
        let read_rel = super::query_rel(&view, "edge").expect("served").expect("ok");
        assert_eq!(read_rel, json!({
            "columns": ["a", "b"],
            "rows": locked_rows,
        }));

        // query_sql
        let sql = format!("SELECT a, b FROM {} ORDER BY a, b", txt_tbl("edge"));
        let locked_sql = json!({"rows": eng.query_sql(&sql, &[]).unwrap()});
        let read_sql = super::query_sql(&view, &sql, &[]).expect("served").expect("ok");
        assert_eq!(read_sql, locked_sql, "query_sql read path == locked path");

        // unknown rel is INVALID_PARAMS on both paths
        let unknown = super::query_rel(&view, "nope").expect("served");
        assert!(matches!(unknown, Err((INVALID_PARAMS, _))));
    }

    /// (b) SLA proof: a read RPC completes well under the time a tick would hold
    /// the engine mutex. A background thread holds `sr.eng` for ~3s; a read call
    /// (which opens its OWN read-only connection and never takes that mutex) must
    /// return correct rows in well under 500ms.
    #[test]
    fn read_path_beats_held_engine_mutex() {
        let (eng, db_str, _dir, prog) = fixture(EDGE);
        let view = view_of(&eng, &prog, &db_str);

        let eng_mutex = Arc::new(Mutex::new(eng));
        let held = eng_mutex.clone();
        let holder = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            std::thread::sleep(Duration::from_secs(3));
        });
        // Give the holder time to actually acquire the lock first.
        std::thread::sleep(Duration::from_millis(100));
        assert!(eng_mutex.try_lock().is_err(), "holder must own the engine mutex");

        let start = Instant::now();
        let read = super::query(&view).expect("served").expect("ok");
        let elapsed = start.elapsed();
        assert_eq!(read["results"][0]["rows"].as_array().unwrap().len(), 6,
            "read returns correct rows while the engine mutex is held");
        tracing::debug!("[SLA] read RPC completed in {elapsed:?} while a 3s engine-lock hold was in flight");
        assert!(elapsed < Duration::from_millis(500),
            "read completed in {elapsed:?} while a 3s engine-lock hold was in flight");

        holder.join().unwrap();
    }

    /// (c) WAL isolation: a read-only connection opened mid-write-transaction
    /// sees the LAST COMMITTED state, not the in-flight write; after the writer
    /// commits, a fresh read sees it.
    #[test]
    fn wal_reader_sees_last_committed_not_in_flight() {
        let (eng, db_str, _dir, prog) = fixture(EDGE);
        let view = view_of(&eng, &prog, &db_str);

        // Baseline: 3 committed edge rows.
        let before = super::query_rel(&view, "edge").expect("served").expect("ok");
        assert_eq!(before["rows"].as_array().unwrap().len(), 3);

        // Open a second WRITER connection and BEGIN an uncommitted insert into
        // the raw rel table (id-space; content already interned by the fixture).
        let writer = crate::db::open(Some(&db_str)).unwrap();
        let a_id = crate::spine::StringId::of("a").sqlite();
        let d_id = crate::spine::StringId::of("d").sqlite();
        writer.begin_immediate().unwrap();
        writer.exec_params(
            "edge",
            &format!("INSERT INTO {} (a, b) VALUES (?1, ?2)", crate::lower::tbl("edge")),
            &[a_id.into(), d_id.into()],
        ).unwrap();

        // A read opened now must still see only the 3 committed rows.
        let mid = super::query_rel(&view, "edge").expect("served").expect("ok");
        assert_eq!(mid["rows"].as_array().unwrap().len(), 3,
            "reader must not see the in-flight uncommitted insert");

        writer.commit().unwrap();

        // A fresh read after commit sees the 4th row.
        let after = super::query_rel(&view, "edge").expect("served").expect("ok");
        assert_eq!(after["rows"].as_array().unwrap().len(), 4,
            "reader sees the row once the writer commits");

        drop(eng);
    }
}

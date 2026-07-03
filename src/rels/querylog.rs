//! Server-request history. Every daemon `query`/`query_sql` RPC and LSP
//! `dl/query` request appends one row to `_query_log` via `Engine::log_query`
//! at the handler (`src/daemon.rs`, `src/lsp.rs`) — a single-row write through
//! the plural `Db` seam, never a per-row loop. This built-in projects that meta
//! table into the queryable `query_log` relation, the same shape as the
//! `_stmt_ms` -> `stmt_ms` projection in `perf.rs`.
//!
//! Append-only, no retention: a polling reader (the flow panel's auto-refresh)
//! querying `query_log` accumulates its own read as a new row too — intentional
//! self-noise. The refresh below reads/writes the engine's `Db` directly and
//! never calls `Engine::query_sql`, so reading `query_log` cannot recurse into
//! logging a query against itself.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::tbl;

use super::{col, RelKind};

pub struct QueryLogKind;

type Row = (String, String, String, String, String);

impl RelKind for QueryLogKind {
    fn rels(&self) -> &'static [&'static str] {
        &["query_log"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "query_log".into(),
            cols: vec![
                col("ts", Type::Text),
                col("source", Type::Text),
                col("method", Type::Text),
                col("body", Type::Text),
                col("params", Type::Text),
            ],
            group: "daemon",
            doc: "history of server query requests: one row per daemon `query`/`query_sql` RPC and LSP `dl/query` request (ts = ISO-8601 UTC, source in {daemon,lsp}, method = RPC name, body = SQL text or empty, params = JSON array text); append-only, no retention — a polling client (the flow panel) accumulates its own rows too, by design",
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in server-query-history relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let want: Vec<Row> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(
                "SELECT ts, source, method, body, params FROM _query_log ORDER BY rowid")?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, String>(3)?, r.get::<_, String>(4)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let have: Vec<Row> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"ts\", \"source\", \"method\", \"body\", \"params\" FROM {} ORDER BY rowid",
                tbl("query_log")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, String>(3)?, r.get::<_, String>(4)?,
            )))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if have == want { return Ok(false); }
        let rows: Vec<Vec<Value>> = want.into_iter()
            .map(|(ts, source, method, body, params)| vec![
                Value::Text(ts), Value::Text(source), Value::Text(method),
                Value::Text(body), Value::Text(params),
            ])
            .collect();
        eng.refresh_rel("query_log", &["ts", "source", "method", "body", "params"], &rows)?;
        Ok(true)
    }
}

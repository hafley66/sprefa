//! Engine-telemetry relations: `rel_count`, `stmt_ms`. The tick already knows
//! its own cardinalities (`--tick-audit` prints them) and its own statement
//! costs (`--profile` logs them); these rels turn that telemetry into FACTS so
//! a perf rail is an ordinary `.dl` rule on the diag/--check seam instead of a
//! human reading a log. Origin: the flow-interproc audit — a 471k-row derived
//! rel and a 40s join statement were both visible in the logs and invisible to
//! every rail (see examples/perf-rails.dl).

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::tbl;

use super::{col, RelKind};

/// Row count per declared relation, as of this refresh. Honest limits: the
/// refresh runs in the tick's SOURCE phase, so derived rels report the
/// PREVIOUS tick's counts (one-tick lag, same as any demand rel — under the
/// daemon a rail converges next tick). The family's own rels are excluded so
/// the self-diff cannot oscillate on its own output.
pub struct PerfKind;

impl RelKind for PerfKind {
    fn rels(&self) -> &'static [&'static str] {
        &["rel_count", "stmt_ms"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl {
                name: "rel_count".into(),
                cols: vec![col("rel", Type::Text), col("rows", Type::Int)],
                ..Default::default()
            },
            RelDecl {
                name: "stmt_ms".into(),
                cols: vec![col("rel", Type::Text), col("ms", Type::Int)],
                ..Default::default()
            },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in engine-telemetry relation (rel_count / stmt_ms)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        // rel_count: COUNT(*) per declared rel. Own family excluded (the
        // self-diff must not oscillate on its own output). VIEWS excluded: a
        // closure head is a reachability view, and COUNT(*) on it materializes
        // the full closure — the exact unbounded evaluation the closure-query
        // guard refuses. Only real tables are counted.
        let views: std::collections::HashSet<String> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'view'")?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let mut counts: Vec<(String, i64)> = Vec::new();
        for rel in eng.rels.keys() {
            if self.rels().contains(&rel.as_str()) || views.contains(&tbl(rel)) { continue; }
            let n: i64 = eng.db.conn().query_row(
                &format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
            counts.push((rel.clone(), n));
        }
        counts.sort();
        // stmt_ms: project `_stmt_ms` (written batched by rebuild_derived) for
        // rels still declared — a rel dropped from the program drops its row.
        // Empty until the FIRST derived rebuild lands in this db, so a one-shot
        // CLI run reports on the second invocation; the daemon on every tick
        // after its first.
        let mut timings: Vec<(String, i64)> = Vec::new();
        {
            let conn = eng.db.conn();
            let mut s = conn.prepare("SELECT rel, ms FROM _stmt_ms ORDER BY rel")?;
            let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows.flatten() {
                if eng.rels.contains_key(&row.0) { timings.push(row); }
            }
        }
        let read_pairs = |rel: &str, c0: &str, c1: &str| -> Result<Vec<(String, i64)>> {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"{c0}\", \"{c1}\" FROM {} ORDER BY \"{c0}\"", tbl(rel)))?;
            let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            Ok(rows.filter_map(|x| x.ok()).collect())
        };
        let mut changed = false;
        if read_pairs("rel_count", "rel", "rows")? != counts {
            let rows: Vec<Vec<Value>> = counts.into_iter()
                .map(|(r, n)| vec![Value::Text(r), Value::Int(n)]).collect();
            eng.refresh_rel("rel_count", &["rel", "rows"], &rows)?;
            changed = true;
        }
        if read_pairs("stmt_ms", "rel", "ms")? != timings {
            let rows: Vec<Vec<Value>> = timings.into_iter()
                .map(|(r, ms)| vec![Value::Text(r), Value::Int(ms)]).collect();
            eng.refresh_rel("stmt_ms", &["rel", "ms"], &rows)?;
            changed = true;
        }
        Ok(changed)
    }
}

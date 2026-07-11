//! Engine-telemetry relations: `rel_count`, `stmt_ms`. The tick already knows
//! its own cardinalities (`--tick-audit` prints them) and its own statement
//! costs (`--profile` logs them); these rels turn that telemetry into FACTS so
//! a perf rail is an ordinary `.dl` rule on the diag/--check seam instead of a
//! human reading a log. Origin: the flow-interproc audit — a 471k-row derived
//! rel and a 40s join statement were both visible in the logs and invisible to
//! every rail (see examples/perf-rails.dl).

use std::collections::BTreeMap;

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::desugar::display_rel_name;
use crate::engine::Engine;
use crate::lower::tbl;

use super::{col, RelKind};

/// Fold a mixed rel's hidden `__src`/`__drv` twins (see `engine::desugar`)
/// under the visible name the program declared, summing the numeric column,
/// so neither telemetry rel ever surfaces a twin name (D4). Non-mixed rels
/// pass through unchanged (`display_rel_name` is a no-op for them). Output is
/// sorted by name so callers see deterministic order.
fn fold_twins(rows: Vec<(String, i64)>) -> Vec<(String, i64)> {
    let mut merged: BTreeMap<String, i64> = BTreeMap::new();
    for (rel, value) in rows {
        *merged.entry(display_rel_name(&rel).to_string()).or_insert(0) += value;
    }
    merged.into_iter().collect()
}

/// `fold_twins` for the (ms, n) pair shape of `stmt_ms`: both numeric columns
/// sum under the visible rel name. A prefixed engine bucket (`closure:x`)
/// contains a colon, which `display_rel_name` leaves untouched.
fn fold_twin_triples(rows: Vec<(String, i64, i64)>) -> Vec<(String, i64, i64)> {
    let mut merged: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for (rel, ms, n) in rows {
        let e = merged.entry(display_rel_name(&rel).to_string()).or_insert((0, 0));
        e.0 += ms;
        e.1 += n;
    }
    merged.into_iter().map(|(rel, (ms, n))| (rel, ms, n)).collect()
}

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
                group: "perf",
                doc: "row count per declared relation at refresh time; derived rels report the previous tick's counts (source-phase refresh, one-tick lag) — the cardinality-blowup rail joins here",
                ..Default::default()
            },
            RelDecl {
                name: "stmt_ms".into(),
                cols: vec![col("rel", Type::Text), col("ms", Type::Int), col("n", Type::Int)],
                group: "perf",
                doc: "statement cost of each derived rel's most recent rebuild: ms = SUM of wall ms across its rules/passes/delta variants, n = how many statement executions that sum covers (n=1 is one big join, large n is many fixpoint passes); prefixed sibling buckets (closure:<edge> / cond_cache:<edge> / extract:<rel> / closure_seed:<rel> / scc:<rel> / node2vec:<edge>) time the engine-side derived work; empty until a rebuild has landed in this db, so a one-shot CLI run reports on the second invocation — the slow-rule rail joins here",
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
        let counts = fold_twins(counts);
        // stmt_ms: project `_stmt_ms` (written batched by rebuild_derived) for
        // rels still declared — a rel dropped from the program drops its row.
        // Empty until the FIRST derived rebuild lands in this db, so a one-shot
        // CLI run reports on the second invocation; the daemon on every tick
        // after its first.
        // A prefixed key (`closure:<edge>`, `scc:<rel>`, ...) is an engine-side
        // bucket for a declared rel's derived-phase work; it survives the
        // declared-rel filter as long as its suffix rel is still declared.
        let mut timings: Vec<(String, i64, i64)> = Vec::new();
        {
            let conn = eng.db.conn();
            let mut s = conn.prepare("SELECT rel, ms, n FROM _stmt_ms ORDER BY rel")?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
            for row in rows.flatten() {
                let name = row.0.split_once(':').map(|(_, suffix)| suffix).unwrap_or(&row.0);
                if eng.rels.contains_key(name) { timings.push(row); }
            }
        }
        let timings = fold_twin_triples(timings);
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
        let read_triples = || -> Result<Vec<(String, i64, i64)>> {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"rel\", \"ms\", \"n\" FROM {} ORDER BY \"rel\"", tbl("stmt_ms")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
            Ok(rows.filter_map(|x| x.ok()).collect())
        };
        if read_triples()? != timings {
            let rows: Vec<Vec<Value>> = timings.into_iter()
                .map(|(r, ms, n)| vec![Value::Text(r), Value::Int(ms), Value::Int(n)]).collect();
            eng.refresh_rel("stmt_ms", &["rel", "ms", "n"], &rows)?;
            changed = true;
        }
        Ok(changed)
    }
}

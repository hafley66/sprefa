//! Family-derive reactive engine — host extraction families as pure `derive`
//! functions over owned input tables, with the engine owning dep capture,
//! affected-set computation, and reconcile. See
//! `plans/2026-07-15-family-derive-reactive-engine.md`.
//!
//! Step 0 surface: the `Family` trait, a SQLite-backed `Ctx` that records a
//! dep per input row read (by stable integer PK), a `RowSink`, and a
//! `derive_family` runner. The first hosted family is the projection
//! `call_site` ([`call_site::CallSite`]); the aggregation tier
//! (`call_edge` / support) is step 2.

use anyhow::Result;
use std::collections::HashSet;

use crate::ast::Value;
use crate::db::Db;

pub(crate) mod call_site;
pub(crate) mod call_edge;
pub(crate) mod router;

pub(crate) use call_edge::CallEdge;
pub(crate) use call_site::CallSite;
pub(crate) use router::FamilyRouter;

/// The call family's hosted relations, as `'static` refs so a persistent
/// `FamilyRouter` can hold them across engine ticks. Unit structs, so the
/// statics are zero-sized.
static CALL_SITE: CallSite = CallSite;
static CALL_EDGE: CallEdge = CallEdge;

/// The families the reactive call-rel flip routes through, in the order the
/// public rels are written. Declaration order = `react`'s return order.
pub(crate) fn call_families() -> Vec<&'static dyn Family> {
    vec![&CALL_SITE, &CALL_EDGE]
}

/// The `_call_*` input relations every call family reads on a full refresh —
/// the changed-set passed to `react` when the whole owned baseline was
/// rewritten (nothing to skip). A delta path passes the subset it touched.
pub(crate) fn call_input_rels() -> std::collections::HashSet<&'static str> {
    ["_call_owner", "_call_raw_site", "_call_resolution"].into_iter().collect()
}

/// A captured input dependency: the relation read plus the stable integer
/// primary key of the row read. The engine asks "did any row a family read
/// move?" by intersecting these with a delta's changed keys. Keyed by PK,
/// not vector index, so identity survives reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DepKey {
    pub rel: &'static str,
    pub pk: i64,
}

/// One emitted output row (a public-relation tuple as `Value` cells).
pub(crate) type OutRow = Vec<Value>;

/// Sink a family emits into. Plain Vec for step 0; the engine wraps reconcile
/// (diff old vs new, transactional retract/insert) around this in later steps.
pub(crate) struct RowSink {
    pub rows: Vec<OutRow>,
}

/// Tracked read context. Every `scan` records a `DepKey` per row read, so the
/// engine learns a family's inputs by intercepting reads (the MobX/SolidJS
/// `computed` model), not from a declared dep array (the React `useMemo`
/// model, which undercaptures — the alias-bug class).
pub(crate) struct Ctx<'a> {
    db: &'a Db,
    deps: HashSet<DepKey>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(db: &'a Db) -> Self {
        Self { db, deps: HashSet::new() }
    }

    /// Scan an internal table: return each row's integer PK plus the requested
    /// columns as `Value` cells. Records a `DepKey { rel, pk }` for every row
    /// returned. Columns are read as `Option<i64>`; NULL becomes `Value::Null`
    /// (the `_call_*` schema is `NOT NULL` on the PK and most sid columns, but
    /// `classification_sid` and `unique_sym_sid` are nullable).
    pub(crate) fn scan(
        &mut self,
        rel: &'static str,
        pk_col: &str,
        cols: &[&str],
    ) -> Result<Vec<(i64, OutRow)>> {
        let mut col_list = String::with_capacity(pk_col.len() + cols.len() * 8);
        col_list.push_str(pk_col);
        for c in cols {
            col_list.push(',');
            col_list.push_str(c);
        }
        let sql = format!("SELECT {col_list} FROM {rel}");
        let mut stmt = self.db.prepare(&sql)?;
        let rows: Vec<(i64, OutRow)> = stmt
            .query_map([], |row| {
                let pk: i64 = row.get(0)?;
                let mut out = Vec::with_capacity(cols.len());
                for i in 0..cols.len() {
                    let cell: Option<i64> = row.get(i + 1)?;
                    out.push(cell.map(Value::Int).unwrap_or(Value::Null));
                }
                Ok((pk, out))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (pk, _) in &rows {
            self.deps.insert(DepKey { rel, pk: *pk });
        }
        Ok(rows)
    }
}

/// A derived relation. The family declares its name and writes one pure
/// `derive` body that reads inputs through `Ctx` and emits output rows. No
/// delta method, no reproject, no preflight: the engine owns those.
pub(crate) trait Family: Send + Sync {
    fn name(&self) -> &'static str;
    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()>;
}

/// Run one family's derive against `db`, returning its output rows and the
/// input dependencies it captured. Cold-load and rederive use the same path;
/// the difference is only which inputs are present.
pub(crate) fn derive_family(db: &Db, family: &dyn Family) -> Result<(Vec<OutRow>, HashSet<DepKey>)> {
    let mut ctx = Ctx::new(db);
    let mut sink = RowSink { rows: Vec::new() };
    family.derive(&mut ctx, &mut sink)?;
    Ok((sink.rows, ctx.deps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::engine::Engine;
    use std::fs;
    use std::path::PathBuf;

    /// Minimal Rust: `beta` calls `alpha` so both `call_site` and the resolved
    /// `call_edge` are non-empty after a real extraction.
    const RUST_SRC: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
";

    fn fresh_engine(root: &PathBuf) -> Engine {
        let mut engine = Engine::new(db::open(None).unwrap(), root.clone());
        engine.ensure_meta().unwrap();
        engine.declare_builtins().unwrap();
        engine
            .db
            .conn()
            .execute(
                "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
                 VALUES ('', 'lib.rs', 'WORK', '', 0, 0)",
                [],
            )
            .unwrap();
        engine
    }

    fn snapshot(engine: &Engine) -> (Vec<[i64; 5]>, Vec<[i64; 3]>) {
        let site: Vec<[i64; 5]> = {
            let mut s = engine
                .db
                .conn()
                .prepare("SELECT repo, caller, callee, file, line FROM rel_call_site")
                .unwrap();
            let mut v: Vec<[i64; 5]> = s
                .query_map([], |row| {
                    Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?])
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            v.sort();
            v
        };
        let edge: Vec<[i64; 3]> = {
            let mut s = engine
                .db
                .conn()
                .prepare("SELECT caller, callee, kind FROM rel_call_edge")
                .unwrap();
            let mut v: Vec<[i64; 3]> = s
                .query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            v.sort();
            v
        };
        (site, edge)
    }

    /// The real-extraction proof: a genuine `refresh_call_rels` over Rust
    /// source on disk, run twice over identical input — once with
    /// `DL_FAMILY_CALL` unset (legacy projection) and once with it set (the
    /// family flip gate fires `family_overwrite_call_rels`). The public
    /// `rel_call_site` + `rel_call_edge` must be identical.
    #[test]
    fn family_flag_matches_legacy_on_real_extraction() {
        let dir = std::env::temp_dir().join(format!(
            "sprf-family-flip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

        // Engine A: flag OFF — legacy projection is the live producer.
        let mut a = fresh_engine(&dir);
        a.refresh_call_rels().unwrap();
        let (legacy_site, legacy_edge) = snapshot(&a);

        // Engine B: flag ON — the gate in refresh_call_rels fires and the
        // family path overwrites rel_call_site + rel_call_edge from _call_*.
        std::env::set_var("DL_FAMILY_CALL", "1");
        let mut b = fresh_engine(&dir);
        b.refresh_call_rels().unwrap();
        std::env::remove_var("DL_FAMILY_CALL");

        let (family_site, family_edge) = snapshot(&b);
        let _ = fs::remove_dir_all(&dir);

        assert!(
            !legacy_site.is_empty(),
            "extraction produced no call_site rows; the rail is vacuous"
        );
        assert!(
            !legacy_edge.is_empty(),
            "extraction produced no resolved call_edge rows; the rail is vacuous"
        );
        assert_eq!(
            family_site, legacy_site,
            "DL_FAMILY_CALL=1 rel_call_site diverged from legacy on real extraction"
        );
        assert_eq!(
            family_edge, legacy_edge,
            "DL_FAMILY_CALL=1 rel_call_edge diverged from legacy on real extraction"
        );

        // The persistent router: refresh_call_rels above cold-derived both
        // families into engine B's cross-tick memo. Drive the LIVE flip method
        // with a _call_resolution-only changed-set: call_edge reads that table,
        // call_site does not, so a genuine skip must fall out — and it only can
        // if the memo survived the refresh tick (an empty memo would rerun both
        // via react's None-branch). This exercises the real Engine method +
        // persistent RefCell memo + the router's skip, end to end.
        let mut resolution_only = HashSet::new();
        resolution_only.insert("_call_resolution");
        let rerun = b.flip_call_rels_via_router(&resolution_only).unwrap();
        assert_eq!(
            rerun,
            vec!["call_edge"],
            "live persistent router must skip call_site on a _call_resolution-only change"
        );
        let (after_site, after_edge) = snapshot(&b);
        assert_eq!(after_site, legacy_site, "skipped call_site must be untouched");
        assert_eq!(after_edge, legacy_edge, "reran call_edge must stay correct");
    }
}

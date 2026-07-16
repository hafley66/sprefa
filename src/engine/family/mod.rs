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

pub(crate) mod call_def;
pub(crate) mod call_def_rev;
pub(crate) mod call_site;
pub(crate) mod call_edge;
pub(crate) mod call_edge_rev;
pub(crate) mod call_kind;
pub(crate) mod call_name;
pub(crate) mod router;

pub(crate) use call_edge::CallEdge;
pub(crate) use call_name::CallName;
pub(crate) use call_site::CallSite;
pub(crate) use router::FamilyRouter;

/// The families the reactive call-rel flip routes through, collected from the
/// self-registration inventory (`register_family!` in each family file), sorted
/// by name so the derive/return order is deterministic across builds. Adding a
/// family touches no code here — it submits itself.
pub(crate) fn call_families() -> Vec<&'static dyn Family> {
    let mut families: Vec<&'static dyn Family> =
        inventory::iter::<FamilyReg>().map(|reg| reg.0).collect();
    families.sort_by_key(|family| family.name());
    families
}

/// Every internal input relation any call family reads — the changed-set passed
/// to `react` on a full refresh, when the whole owned baseline was rewritten
/// (nothing to skip). Framework-computed as the union of each family's
/// self-declared `input_rels`; a delta path passes only the subset it touched.
pub(crate) fn call_input_rels() -> std::collections::HashSet<&'static str> {
    call_families()
        .iter()
        .flat_map(|family| family.input_rels().iter().copied())
        .collect()
}

/// The exact write footprint of an owner/site delta (`apply_call_owner_delta`):
/// it rewrites the owner row + its sites + their resolutions, but leaves
/// `_call_def` alone because the def set is unchanged (the delta bails
/// otherwise). Feeding this to `react` reruns `CallSite`/`CallEdge` and SKIPS
/// `CallName`, whose footprint is `{_call_def}` — the live, non-latent skip.
pub(crate) fn call_owner_delta_rels() -> std::collections::HashSet<&'static str> {
    ["_call_owner", "_call_raw_site", "_call_resolution"]
        .into_iter()
        .collect()
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

/// Sink a family emits into. Plain Vec; the router wraps [`reconcile`] around
/// its output to turn a fresh derivation into a [`RowDelta`].
pub(crate) struct RowSink {
    pub rows: Vec<OutRow>,
}

/// A row-level output delta: the rows to retract and the rows to insert to move
/// a relation from its memoized prior state to a fresh derivation. This is the
/// reconcile/render unit — the engine applies it incrementally (retract old +
/// insert new) rather than overwriting the whole relation, so a RETRACTED input
/// row propagates to a retracted output row instead of being silently rebuilt.
#[derive(Debug, Default)]
pub(crate) struct RowDelta {
    pub retracted: Vec<OutRow>,
    pub inserted: Vec<OutRow>,
}

impl RowDelta {
    pub(crate) fn is_empty(&self) -> bool {
        self.retracted.is_empty() && self.inserted.is_empty()
    }
}

/// Type-tagged string identity for one derived row: distinguishes `Int(0)` from
/// `Text("0")` from `Null` so the diff never conflates cells across types. These
/// are set-valued relations, so the whole tuple is the identity.
fn row_key(row: &OutRow) -> String {
    let mut key = String::new();
    for cell in row {
        match cell {
            Value::Int(n) => {
                key.push('i');
                key.push_str(&n.to_string());
            }
            Value::Text(s) => {
                key.push('t');
                key.push_str(s);
            }
            Value::Null => key.push('n'),
        }
        key.push('\u{1}');
    }
    key
}

/// Diff a fresh derivation `new` against the memoized `old`, returning the
/// minimal retract+insert delta by full-tuple identity. Deduplicates `new`
/// (set semantics). Empty delta = the derivation is unchanged.
pub(crate) fn reconcile(old: &[OutRow], new: Vec<OutRow>) -> RowDelta {
    let old_keys: HashSet<String> = old.iter().map(row_key).collect();
    let mut new_keys: HashSet<String> = HashSet::with_capacity(new.len());
    let mut inserted = Vec::new();
    for row in new {
        let key = row_key(&row);
        let fresh = new_keys.insert(key.clone());
        if fresh && !old_keys.contains(&key) {
            inserted.push(row);
        }
    }
    let retracted = old
        .iter()
        .filter(|row| !new_keys.contains(&row_key(row)))
        .cloned()
        .collect();
    RowDelta { retracted, inserted }
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

/// A derived relation. The family declares its name, its output columns, the
/// input relations it reads, and writes one pure `derive` body that reads
/// inputs through `Ctx` and emits output rows. No delta method, no reproject,
/// no preflight, no central registration: the engine owns those, and the
/// family self-registers with `register_family!`.
///
/// The four declared slots are the whole authoring surface. `out_cols` lets the
/// render write the public rel generically (no per-family `match name` arm);
/// `input_rels` lets the changed-set union be framework-computed (no
/// hand-maintained `call_input_rels`). This is the v3 min-author-ops
/// `(1,0,0)` shape: a new family is one file, zero central edits.
pub(crate) trait Family: Send + Sync {
    fn name(&self) -> &'static str;
    /// The public relation's column names, in emit order. The render writes
    /// `tbl(name)` from these, so adding a family needs no routing-arm edit.
    fn out_cols(&self) -> &'static [&'static str];
    /// Every internal relation this family's `derive` scans. The engine unions
    /// these across families to build the full changed-set for a cold refresh.
    fn input_rels(&self) -> &'static [&'static str];
    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()>;
}

/// Self-registration cell. Each family file emits one `register_family!(TYPE)`
/// (a wrapped `inventory::submit!`), so the family set is collected at binary
/// init with zero central edits. `call_families` iterates this registry.
pub(crate) struct FamilyReg(pub &'static dyn Family);
inventory::collect!(FamilyReg);

/// Register a zero-sized family unit struct into the [`FamilyReg`] inventory.
/// `&$ty` const-promotes to `'static` (the struct is a unit literal), then
/// unsizes to `&'static dyn Family`.
macro_rules! register_family {
    ($ty:ident) => {
        inventory::submit! { $crate::engine::family::FamilyReg(&$ty) }
    };
}
pub(crate) use register_family;

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

    /// A fresh engine rooted at `dir` (which must already hold `lib.rs`) with a
    /// `_file` row carrying `hash` as its fact digest. The delta path reads the
    /// `hash` column as the owner's fact digest, so a caller drives a real
    /// working-tree edit by rewriting `lib.rs` on disk and bumping this column.
    fn engine_at(dir: &PathBuf, hash: &str) -> Engine {
        let mut engine = Engine::new(db::open(None).unwrap(), dir.clone());
        engine.ensure_meta().unwrap();
        engine.declare_builtins().unwrap();
        engine
            .db
            .conn()
            .execute(
                "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
                 VALUES ('', 'lib.rs', 'WORK', ?1, 0, 0)",
                [hash],
            )
            .unwrap();
        engine
    }

    fn names(engine: &Engine) -> Vec<[i64; 2]> {
        let mut s = engine
            .db
            .conn()
            .prepare("SELECT sym, name FROM rel_call_name")
            .unwrap();
        let mut v: Vec<[i64; 2]> = s
            .query_map([], |row| Ok([row.get(0)?, row.get(1)?]))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        v.sort();
        v
    }

    fn make_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sprf-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
    /// source on disk populates `rel_call_site`/`rel_call_edge` non-vacuously
    /// through the family router — the SOLE writer of every public call rel
    /// (P4, capstone cutover; there is no more legacy projection to diff
    /// against).
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

        let mut engine = fresh_engine(&dir);
        engine.refresh_call_rels().unwrap();
        let (site, edge) = snapshot(&engine);
        let _ = fs::remove_dir_all(&dir);

        assert!(
            !site.is_empty(),
            "extraction produced no call_site rows; the rail is vacuous"
        );
        assert!(
            !edge.is_empty(),
            "extraction produced no resolved call_edge rows; the rail is vacuous"
        );

        // The persistent router: refresh_call_rels above cold-derived every
        // family into the engine's cross-tick memo. Drive the LIVE flip method
        // with a _call_resolution-only changed-set: call_edge/call_edge_rev
        // read that table, call_site does not, so a genuine skip must fall
        // out — and it only can if the memo survived the refresh tick (an
        // empty memo would rerun everything via react's None-branch). This
        // exercises the real Engine method + persistent RefCell memo + the
        // router's skip, end to end.
        let mut resolution_only = HashSet::new();
        resolution_only.insert("_call_resolution");
        let rerun = engine.flip_call_rels_via_router(&resolution_only).unwrap();
        assert_eq!(
            rerun,
            vec!["call_edge", "call_edge_rev"],
            "live persistent router reruns the two _call_resolution readers and skips \
             call_site/call_kind/call_name on a _call_resolution-only change"
        );
        let (after_site, after_edge) = snapshot(&engine);
        assert_eq!(after_site, site, "skipped call_site must be untouched");
        assert_eq!(after_edge, edge, "reran call_edge must stay correct");
    }

    /// The live-skip proof on a REAL delta (not a hand-passed changed-set): a
    /// working-tree edit that removes ONE call site while leaving every
    /// definition intact. That is exactly the shape the owner-delta fast path
    /// accepts (`apply_call_owner_delta` requires the def digest unchanged), and
    /// it rewrites `_call_owner`/`_call_raw_site`/`_call_resolution` but never
    /// `_call_def`. So the router, fed the delta's real footprint, reruns
    /// `CallSite`/`CallEdge` and SKIPS `CallName` — the skip pays off in
    /// production, not just in a synthetic changed-set.
    #[test]
    fn family_delta_skips_call_name_on_site_only_change() {
        use crate::rels::PathRefreshContext;

        // v1 -> v2 retargets gamma's second call (`alpha()` -> `beta()`). The
        // edit is line-count preserving, so every def's (line, end) span — hence
        // the owner def digest — is byte-identical and the owner-delta fast path
        // accepts it. call_site/call_edge lose the gamma->alpha tuple; call_name
        // (the def-name projection) is unchanged.
        const V1: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
";
        const V2: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    beta();
}
";

        // Reference expectation for v2: a fresh engine that extracts v2 directly
        // (no delta path involved) — the oracle the delta-path result below must
        // match.
        let reference_dir = make_dir("family-delta-reference");
        fs::write(reference_dir.join("lib.rs"), V2).unwrap();
        let mut reference = engine_at(&reference_dir, "v2");
        reference.refresh_call_rels().unwrap();
        let (reference_site_v2, reference_edge_v2) = snapshot(&reference);
        let reference_name_v2 = names(&reference);
        let _ = fs::remove_dir_all(&reference_dir);
        assert!(!reference_name_v2.is_empty(), "fixture: v2 has def names");

        // Engine under test: full refresh over v1 builds the baseline AND the
        // persistent router memo (every family cold-derived).
        let dir = make_dir("family-delta");
        fs::write(dir.join("lib.rs"), V1).unwrap();
        let mut engine = engine_at(&dir, "v1");
        engine.refresh_call_rels().unwrap();
        let (site_v1, _edge_v1) = snapshot(&engine);
        let name_v1 = names(&engine);
        assert_eq!(name_v1, reference_name_v2, "defs unchanged across v1/v2");
        assert!(!name_v1.is_empty(), "fixture: v1 has def names");

        // Real working-tree edit: rewrite the file and bump its fact digest.
        fs::write(dir.join("lib.rs"), V2).unwrap();
        engine.db
            .conn()
            .execute("UPDATE _file SET hash = 'v2' WHERE path = 'lib.rs'", [])
            .unwrap();

        // Drive the genuine incremental path.
        let mut changed_paths = std::collections::HashSet::new();
        changed_paths.insert("lib.rs".to_string());
        let context = PathRefreshContext {
            changed_paths: &changed_paths,
            module_dependency_changed: false,
            module_full_refresh: false,
        };
        let outcome = engine.refresh_call_rels_delta(&context).unwrap();
        assert_eq!(
            outcome,
            crate::rels::CallPathRefreshOutcome::Applied,
            "site-only delta must take the owner-delta fast path",
        );

        // (iii) the reran families are byte-identical to a reference v2 extraction.
        let (site_after, edge_after) = snapshot(&engine);
        assert_eq!(site_after, reference_site_v2, "reran call_site must match reference v2");
        assert_eq!(edge_after, reference_edge_v2, "reran call_edge must match reference v2");
        // The change was real: the gamma->alpha site is gone.
        assert_ne!(site_after, site_v1, "delta must have altered call_site");

        // (ii) the SKIPPED family's public rel is untouched by the delta: the
        // router leaves call_name alone, so it still holds the v1 rows (equal
        // to v2, since defs are identical).
        assert_eq!(names(&engine), name_v1, "skipped call_name rows must be unchanged");

        // (i) the skip itself: replay the router with the delta's real footprint
        // (`react` is a pure function of memo footprints + changed-set, so the
        // replay's rerun set equals what the live delta just used). call_name is
        // absent -> CallName was skipped, not rerun-to-the-same-rows.
        let rerun = engine
            .flip_call_rels_via_router(&crate::engine::family::call_owner_delta_rels())
            .unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            rerun,
            vec!["call_edge", "call_edge_rev", "call_kind", "call_site"],
            "owner-delta footprint must rerun call_site/call_edge/call_edge_rev/call_kind \
             (every _call_raw_site reader) and SKIP call_name (footprint {{_call_def}})",
        );
    }
}

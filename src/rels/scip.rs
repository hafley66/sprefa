//! SCIP-importer relations, loaded from an existing `index.scip` (reload-gated).

use anyhow::Result;
use std::collections::HashSet;

use crate::ast::{Program, RelDecl, Type, Value};
use crate::engine::{rels_used, scip_descriptor_name, Engine};
use crate::scip_import;

use super::{col, ExtractFamily, RelKind};

// --- scip (importer, reload-gated) -------------------------------------------

/// SCIP-importer relations, loaded from an existing `index.scip`.
/// `scip_def(symbol, file)` / `scip_ref(file, symbol, def_file)` /
/// `scip_edge(src, dst)` are the file-level def/ref/import graph;
/// `scip_name(symbol, name)` is the descriptor's trailing identifier (computed
/// where the moniker grammar lives — a pure-dl split can't isolate it);
/// `scip_fn_edge(caller, callee)` is the function-level call graph;
/// `scip_callee_type(sym, type)` maps a method moniker to its receiver type;
/// `scip_local(fn, name)` the locals; `scip_impl(impl, iface)` the
/// implementation edges. Unlike the self-diffing families, the importer always
/// re-emits when run, so `dirty` gates an incremental tick on `index.scip`
/// itself moving.
///
/// Lazy multi-repo tier: a user-DERIVED relation named `scip_want(repo)` (the
/// `org`-allowlist convention — the engine reads it, users head it, e.g.
/// `scip_want(r) <- repo(r, _, _), lang(r, "go").`) demands an index per named
/// repo. Each wanted root gets `scip_setup::ensure_index` (existing index wins;
/// otherwise detected+installed indexers run once to `.dl/index.scip`), the
/// self index plus all wanted indexes merge via `merge_files`, and ONE load
/// fills the same rels — NO schema change, monikers self-disambiguate across
/// repos, and cross-repo refs resolve because the def map spans the merged
/// document set. Want rows read this tick are last tick's derivations (the
/// data-driven-scan latency contract). A repo with no toolchain skips loudly.
pub struct ScipKind;

impl RelKind for ScipKind {
    fn rels(&self) -> &'static [&'static str] {
        &["scip_def", "scip_name", "scip_ref", "scip_edge",
          "scip_fn_edge", "scip_callee_type", "scip_local", "scip_impl",
          "scip_occurrence", "scip_binding"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "scip_def".into(), cols: vec![col("symbol", Type::Text), col("file", Type::Path), col("repo", Type::Text)], group: "scip",
                doc: "symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX); repo = origin index", ..Default::default() },
            RelDecl { name: "scip_name".into(), cols: vec![col("symbol", Type::Text), col("name", Type::Text)], group: "scip",
                doc: "descriptor name (last identifier run) of a moniker, computed in-engine", ..Default::default() },
            RelDecl { name: "scip_ref".into(), cols: vec![col("file", Type::Path), col("symbol", Type::Text), col("def_file", Type::Path), col("repo", Type::Text)], group: "scip",
                doc: "compiler-backed references (ref file, symbol, def file, origin repo)", ..Default::default() },
            RelDecl { name: "scip_edge".into(), cols: vec![col("src", Type::Path), col("dst", Type::Path), col("repo", Type::Text)], group: "scip",
                doc: "file-to-file SCIP dependency edges (with origin repo)", ..Default::default() },
            RelDecl { name: "scip_fn_edge".into(), cols: vec![col("caller", Type::Text), col("callee", Type::Text)], group: "scip",
                doc: "function-level call edge; caller is the innermost enclosing fn def", ..Default::default() },
            RelDecl { name: "scip_callee_type".into(), cols: vec![col("sym", Type::Text), col("type", Type::Text)], group: "scip",
                doc: "receiver type parsed from a method moniker's impl/for segment", ..Default::default() },
            RelDecl { name: "scip_local".into(), cols: vec![col("fn", Type::Text), col("name", Type::Text)], group: "scip",
                doc: "local-variable + parameter declarations attributed to their enclosing fn", ..Default::default() },
            RelDecl { name: "scip_impl".into(), cols: vec![col("impl", Type::Text), col("iface", Type::Text)], group: "scip",
                doc: "interface/supertype dispatch edge from SCIP is_implementation (impl to iface)", ..Default::default() },
            RelDecl { name: "scip_occurrence".into(), cols: vec![
                    col("file", Type::Path), col("symbol", Type::Text),
                    col("line", Type::Int), col("col", Type::Int),
                    col("end_line", Type::Int), col("end_col", Type::Int),
                    col("role", Type::Text), col("repo", Type::Text)], group: "scip",
                doc: "every SCIP occurrence with its line/col span, role (definition/import/write/read/reference), and origin repo — the position handle scip_ref lacked; line/col: 0-based", ..Default::default() },
            RelDecl { name: "scip_binding".into(), cols: vec![
                    col("file", Type::Path), col("symbol", Type::Text),
                    col("local_name", Type::Text),
                    col("line", Type::Int), col("col", Type::Int),
                    col("repo", Type::Text)], group: "scip",
                doc: "an occurrence's LOCAL binding text (source slice at its range) joined to the canonical symbol — resolves an alias/default import (import { foo as bar }) that scip_name's canonical-only name drops; WORK content slice; line/col: 0-based", ..Default::default() },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in SCIP relation"
    }
    fn pre_extract(&self) -> bool {
        true
    }
    fn used(&self, prog: &Program) -> bool {
        // The type/call resolve closures prefer `scip_ref` when an index
        // exists, so a type/call/doc program forces the load without naming
        // a scip rel — otherwise SPREFA_SCIP_INDEX (or a root index.scip) is
        // a silent no-op for exactly the programs it should improve (the
        // ModuleFamily gate bug, same shape). With no index on disk the
        // refresh is a cheap no-op, so the wider gate costs nothing.
        rels_used(prog, self.rels())
            || super::TypeFamily.used(prog)
            || super::CallFamily.used(prog)
    }
    fn dirty(&self, eng: &Engine, changed: &HashSet<String>) -> bool {
        // Match the root `index.scip` and the `.dl/index.scip` that `dl index`
        // writes (the changed-path string carries the relative prefix).
        if changed.iter().any(|c| c.ends_with("index.scip")) {
            return true;
        }
        // A gitignored index never enters the corpus walk, so the changed set
        // cannot see a re-index; stat the self index directly and compare with
        // the digest `refresh` stored after its last load. Self repo only:
        // statting a wanted repo's index would require `ensure_index`, which
        // may run an indexer — too heavy for a per-tick gate.
        let stored = eng.load_rel_digest(SCIP_INDEX_STAT_KEY).ok().flatten();
        stored != Some(self_index_stat_digest(eng))
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let t = |s: &str| Value::Text(s.to_string());
        // Each input index is loaded independently and its rows tagged with the
        // origin repo, so two roots of the same crate that emit identical
        // (symbol, relative_path) strings stay distinct instead of collapsing on
        // a blind pre-merge (the cross-root symbol-collapse bug). Cross-repo
        // SCIP resolution is intentionally NOT reconstructed: a ref resolves only
        // within its own index's document set, matching the syntactic resolver's
        // per-repo scoping.
        let mut rows_changed = false;
        let inputs = self.index_inputs(eng)?;
        if inputs.is_empty() {
            rows_changed |= eng.refresh_rel("scip_def", &["symbol", "file", "repo"], &[])?;
            rows_changed |= eng.refresh_rel("scip_name", &["symbol", "name"], &[])?;
            rows_changed |= eng.refresh_rel("scip_ref", &["file", "symbol", "def_file", "repo"], &[])?;
            rows_changed |= eng.refresh_rel("scip_edge", &["src", "dst", "repo"], &[])?;
            rows_changed |= eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &[])?;
            rows_changed |= eng.refresh_rel("scip_callee_type", &["sym", "type"], &[])?;
            rows_changed |= eng.refresh_rel("scip_local", &["fn", "name"], &[])?;
            rows_changed |= eng.refresh_rel("scip_impl", &["impl", "iface"], &[])?;
            rows_changed |= eng.refresh_rel("scip_occurrence",
                &["file", "symbol", "line", "col", "end_line", "end_col", "role", "repo"], &[])?;
            rows_changed |= eng.refresh_rel("scip_binding",
                &["file", "symbol", "local_name", "line", "col", "repo"], &[])?;
            eng.save_rel_digest(SCIP_INDEX_STAT_KEY, &self_index_stat_digest(eng))?;
            return Ok(rows_changed);
        }
        let mut all = scip_import::ScipRows::default();
        // Local binding names are computed per-input (the on-disk root is known
        // only inside this loop, before the merge erases which index each
        // occurrence came from) so `scip_binding` slices the RIGHT tree.
        let mut all_bindings: Vec<(String, String, String, i32, i32, String)> = Vec::new();
        for (path, root, slug) in &inputs {
            let rows = scip_import::load(path, root, slug)?;
            all_bindings.extend(scip_import::local_bindings(&rows.occurrences, root));
            all.defs.extend(rows.defs);
            all.refs.extend(rows.refs);
            all.edges.extend(rows.edges);
            all.fn_edges.extend(rows.fn_edges);
            all.callee_types.extend(rows.callee_types);
            all.locals.extend(rows.locals);
            all.occ_spans.extend(rows.occ_spans);
            all.occurrences.extend(rows.occurrences);
            all.impls.extend(rows.impls);
        }
        let rows = all;
        let defs: Vec<Vec<Value>> = rows.defs.iter().map(|(sym, file, repo)| vec![t(sym), t(file), t(repo)]).collect();
        // The symbol's descriptor name (last identifier run), computed where the
        // SCIP moniker grammar lives. A pure-dl `split` chain can't isolate it:
        // `…/impl#[Type]method().` needs the `[`/`]`/`#` separators that single-
        // separator split can't all honor. One row per distinct (symbol, name).
        let mut name_set: HashSet<(String, String)> = HashSet::new();
        for (sym, _, _) in &rows.defs {
            if let Some(name) = scip_descriptor_name(sym) {
                name_set.insert((sym.clone(), name));
            }
        }
        let names: Vec<Vec<Value>> = name_set.iter().map(|(sym, name)| vec![t(sym), t(name)]).collect();
        let refs: Vec<Vec<Value>> = rows.refs.iter()
            .map(|(file, sym, def, repo)| vec![t(file), t(sym), t(def), t(repo)]).collect();
        let edges: Vec<Vec<Value>> = rows.edges.iter().map(|(src, dst, repo)| vec![t(src), t(dst), t(repo)]).collect();
        let fn_edges: Vec<Vec<Value>> = rows.fn_edges.iter()
            .map(|(caller, callee)| vec![t(caller), t(callee)]).collect();
        let callee_types: Vec<Vec<Value>> = rows.callee_types.iter()
            .map(|(sym, ty)| vec![t(sym), t(ty)]).collect();
        let locals: Vec<Vec<Value>> = rows.locals.iter()
            .map(|(fn_, name)| vec![t(fn_), t(name)]).collect();
        let impls: Vec<Vec<Value>> = rows.impls.iter()
            .map(|(im, iface)| vec![t(im), t(iface)]).collect();
        let n = |v: &i32| Value::Int(*v as i64);
        let occurrences: Vec<Vec<Value>> = rows.occurrences.iter()
            .map(|(file, sym, sl, sc, el, ec, role, repo)|
                vec![t(file), t(sym), n(sl), n(sc), n(el), n(ec), t(role), t(repo)])
            .collect();
        let bindings: Vec<Vec<Value>> = all_bindings.iter()
            .map(|(file, sym, name, sl, sc, repo)|
                vec![t(file), t(sym), t(name), n(sl), n(sc), t(repo)])
            .collect();
        rows_changed |= eng.refresh_rel("scip_def", &["symbol", "file", "repo"], &defs)?;
        rows_changed |= eng.refresh_rel("scip_name", &["symbol", "name"], &names)?;
        rows_changed |= eng.refresh_rel("scip_ref", &["file", "symbol", "def_file", "repo"], &refs)?;
        rows_changed |= eng.refresh_rel("scip_edge", &["src", "dst", "repo"], &edges)?;
        rows_changed |= eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &fn_edges)?;
        rows_changed |= eng.refresh_rel("scip_callee_type", &["sym", "type"], &callee_types)?;
        rows_changed |= eng.refresh_rel("scip_local", &["fn", "name"], &locals)?;
        rows_changed |= eng.refresh_rel("scip_impl", &["impl", "iface"], &impls)?;
        rows_changed |= eng.refresh_rel("scip_occurrence",
            &["file", "symbol", "line", "col", "end_line", "end_col", "role", "repo"], &occurrences)?;
        rows_changed |= eng.refresh_rel("scip_binding",
            &["file", "symbol", "local_name", "line", "col", "repo"], &bindings)?;
        eng.save_rel_digest(SCIP_INDEX_STAT_KEY, &self_index_stat_digest(eng))?;
        Ok(rows_changed)
    }
}

/// `_reldigest` key for the self index's stat digest (`scip:` namespace,
/// distinct from row-content `drv:`/`src:` keys).
const SCIP_INDEX_STAT_KEY: &str = "scip:index-stat";

/// Stat digest of the self repo's index file: blake3 over (resolved path,
/// mtime, len), or a fixed "absent" digest when no index resolves. Cheap
/// enough for the per-tick `dirty` gate; content is NOT read — `refresh_rel`'s
/// row diff already dedups a touched-but-identical index downstream.
fn self_index_stat_digest(eng: &Engine) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    match scip_import::index_path(&eng.root) {
        None => {
            h.update(b"absent");
        }
        Some(path) => {
            h.update(path.to_string_lossy().as_bytes());
            if let Ok(md) = std::fs::metadata(&path) {
                h.update(&md.len().to_le_bytes());
                if let Ok(mtime) = md.modified() {
                    if let Ok(d) = mtime.duration_since(std::time::UNIX_EPOCH) {
                        h.update(&d.as_nanos().to_le_bytes());
                    }
                }
            }
        }
    }
    *h.finalize().as_bytes()
}

impl ScipKind {
    /// The index inputs to load this tick, one per repo: `(index_path, on-disk
    /// root, repo slug)`. The self root's `index.scip` (the existing single-repo
    /// path), plus — when a user-derived `scip_want(repo)` demands more repos —
    /// each wanted repo's ensured index. Each is loaded and tagged independently
    /// (`refresh` threads root+slug into `scip_import::load`) so identical
    /// (symbol, relative_path) strings across roots stay distinct rows keyed by
    /// origin repo. No pre-merge — that lost which input each document came from,
    /// collapsing the second root's rows. Empty = no index anywhere (the caller
    /// clears the rels).
    fn index_inputs(&self, eng: &Engine) -> Result<Vec<(std::path::PathBuf, std::path::PathBuf, String)>> {
        let self_slug = eng.self_slug();
        let self_index = scip_import::index_path(&eng.root);
        let mut inputs: Vec<(std::path::PathBuf, std::path::PathBuf, String)> =
            self_index.into_iter().map(|p| (p, eng.root.clone(), self_slug.clone())).collect();
        let want: Vec<String> = match eng.rels.get("scip_want") {
            None => Vec::new(),
            Some(meta) => {
                if meta.cols.is_empty() {
                    anyhow::bail!("scip_want needs a repo column");
                }
                eng.db.query_rows(
                    "scip_want",
                    &format!(
                        "SELECT DISTINCT \"{}\" FROM {} ORDER BY 1",
                        meta.col_name(0), crate::lower::txt_tbl("scip_want")
                    ),
                    &[],
                    |r| Ok(r.get::<_, String>(0)?),
                )?
            }
        };
        if want.is_empty() {
            return Ok(inputs);
        }
        let roots = eng.repo_roots();
        for repo in want {
            let key = match repo.as_str() {
                "." | "" | "self" => self_slug.clone(),
                _ => repo.clone(),
            };
            if key == self_slug { continue; } // self index already an input
            let Some(root) = roots.get(&key) else {
                eprintln!("[scip_want] skip {repo}: unknown repo slug");
                continue;
            };
            if let Some(p) = crate::scip_setup::ensure_index(root)? {
                inputs.push((p, root.clone(), key));
            }
        }
        Ok(inputs)
    }
}

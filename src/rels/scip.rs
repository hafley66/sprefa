//! SCIP-importer relations, loaded from an existing `index.scip` (reload-gated).

use anyhow::Result;
use std::collections::HashSet;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::{scip_descriptor_name, Engine};
use crate::scip_import;

use super::{col, RelKind};

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
          "scip_fn_edge", "scip_callee_type", "scip_local", "scip_impl"]
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
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in SCIP relation"
    }
    fn dirty(&self, changed: &HashSet<String>) -> bool {
        // Match the root `index.scip` and the `.dl/index.scip` that `dl index`
        // writes (the changed-path string carries the relative prefix).
        changed.iter().any(|c| c.ends_with("index.scip"))
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
        let inputs = self.index_inputs(eng)?;
        if inputs.is_empty() {
            eng.refresh_rel("scip_def", &["symbol", "file", "repo"], &[])?;
            eng.refresh_rel("scip_name", &["symbol", "name"], &[])?;
            eng.refresh_rel("scip_ref", &["file", "symbol", "def_file", "repo"], &[])?;
            eng.refresh_rel("scip_edge", &["src", "dst", "repo"], &[])?;
            eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &[])?;
            eng.refresh_rel("scip_callee_type", &["sym", "type"], &[])?;
            eng.refresh_rel("scip_local", &["fn", "name"], &[])?;
            eng.refresh_rel("scip_impl", &["impl", "iface"], &[])?;
            return Ok(true);
        }
        let mut all = scip_import::ScipRows::default();
        for (path, root, slug) in &inputs {
            let rows = scip_import::load(path, root, slug)?;
            all.defs.extend(rows.defs);
            all.refs.extend(rows.refs);
            all.edges.extend(rows.edges);
            all.fn_edges.extend(rows.fn_edges);
            all.callee_types.extend(rows.callee_types);
            all.locals.extend(rows.locals);
            all.occ_spans.extend(rows.occ_spans);
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
        eng.refresh_rel("scip_def", &["symbol", "file", "repo"], &defs)?;
        eng.refresh_rel("scip_name", &["symbol", "name"], &names)?;
        eng.refresh_rel("scip_ref", &["file", "symbol", "def_file", "repo"], &refs)?;
        eng.refresh_rel("scip_edge", &["src", "dst", "repo"], &edges)?;
        eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &fn_edges)?;
        eng.refresh_rel("scip_callee_type", &["sym", "type"], &callee_types)?;
        eng.refresh_rel("scip_local", &["fn", "name"], &locals)?;
        eng.refresh_rel("scip_impl", &["impl", "iface"], &impls)?;
        Ok(true)
    }
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
                let conn = eng.db.conn();
                let mut s = conn.prepare(&format!(
                    "SELECT DISTINCT \"{}\" FROM {} ORDER BY 1",
                    meta.col_name(0), crate::lower::tbl("scip_want")))?;
                let rs = s.query_map([], |r| r.get::<_, String>(0))?;
                rs.filter_map(|x| x.ok()).collect()
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

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
            RelDecl { name: "scip_def".into(), cols: vec![col("symbol", Type::Text), col("file", Type::Path)], group: "scip",
                doc: "symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX)", ..Default::default() },
            RelDecl { name: "scip_name".into(), cols: vec![col("symbol", Type::Text), col("name", Type::Text)], group: "scip",
                doc: "descriptor name (last identifier run) of a moniker, computed in-engine", ..Default::default() },
            RelDecl { name: "scip_ref".into(), cols: vec![col("file", Type::Path), col("symbol", Type::Text), col("def_file", Type::Path)], group: "scip",
                doc: "compiler-backed references (ref file, symbol, def file)", ..Default::default() },
            RelDecl { name: "scip_edge".into(), cols: vec![col("src", Type::Path), col("dst", Type::Path)], group: "scip",
                doc: "file-to-file SCIP dependency edges", ..Default::default() },
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
        let path = self.resolve_index(eng)?;
        let Some(path) = path else {
            eng.refresh_rel("scip_def", &["symbol", "file"], &[])?;
            eng.refresh_rel("scip_name", &["symbol", "name"], &[])?;
            eng.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &[])?;
            eng.refresh_rel("scip_edge", &["src", "dst"], &[])?;
            eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &[])?;
            eng.refresh_rel("scip_callee_type", &["sym", "type"], &[])?;
            eng.refresh_rel("scip_local", &["fn", "name"], &[])?;
            eng.refresh_rel("scip_impl", &["impl", "iface"], &[])?;
            return Ok(true);
        };
        let rows = scip_import::load(&path)?;
        let defs: Vec<Vec<Value>> = rows.defs.iter().map(|(sym, file)| vec![t(sym), t(file)]).collect();
        // The symbol's descriptor name (last identifier run), computed where the
        // SCIP moniker grammar lives. A pure-dl `split` chain can't isolate it:
        // `…/impl#[Type]method().` needs the `[`/`]`/`#` separators that single-
        // separator split can't all honor. One row per distinct (symbol, name).
        let mut name_set: HashSet<(String, String)> = HashSet::new();
        for (sym, _) in &rows.defs {
            if let Some(name) = scip_descriptor_name(sym) {
                name_set.insert((sym.clone(), name));
            }
        }
        let names: Vec<Vec<Value>> = name_set.iter().map(|(sym, name)| vec![t(sym), t(name)]).collect();
        let refs: Vec<Vec<Value>> = rows.refs.iter()
            .map(|(file, sym, def)| vec![t(file), t(sym), t(def)]).collect();
        let edges: Vec<Vec<Value>> = rows.edges.iter().map(|(src, dst)| vec![t(src), t(dst)]).collect();
        let fn_edges: Vec<Vec<Value>> = rows.fn_edges.iter()
            .map(|(caller, callee)| vec![t(caller), t(callee)]).collect();
        let callee_types: Vec<Vec<Value>> = rows.callee_types.iter()
            .map(|(sym, ty)| vec![t(sym), t(ty)]).collect();
        let locals: Vec<Vec<Value>> = rows.locals.iter()
            .map(|(fn_, name)| vec![t(fn_), t(name)]).collect();
        let impls: Vec<Vec<Value>> = rows.impls.iter()
            .map(|(im, iface)| vec![t(im), t(iface)]).collect();
        eng.refresh_rel("scip_def", &["symbol", "file"], &defs)?;
        eng.refresh_rel("scip_name", &["symbol", "name"], &names)?;
        eng.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &refs)?;
        eng.refresh_rel("scip_edge", &["src", "dst"], &edges)?;
        eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &fn_edges)?;
        eng.refresh_rel("scip_callee_type", &["sym", "type"], &callee_types)?;
        eng.refresh_rel("scip_local", &["fn", "name"], &locals)?;
        eng.refresh_rel("scip_impl", &["impl", "iface"], &impls)?;
        Ok(true)
    }
}

impl ScipKind {
    /// The index to load this tick: the self root's `index.scip` alone (the
    /// existing single-repo path), or — when a user-derived `scip_want(repo)`
    /// demands more repos — the self index plus each wanted repo's ensured
    /// index, merged to one temp file so the load resolves refs across repos.
    /// `None` = no index anywhere (the caller clears the rels).
    fn resolve_index(&self, eng: &Engine) -> Result<Option<std::path::PathBuf>> {
        let self_index = scip_import::index_path(&eng.root);
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
            return Ok(self_index);
        }
        let roots = eng.repo_roots();
        let self_slug = eng.self_slug();
        let mut parts: Vec<std::path::PathBuf> = self_index.into_iter().collect();
        for repo in want {
            let key = match repo.as_str() {
                "." | "" | "self" => self_slug.clone(),
                _ => repo.clone(),
            };
            if key == self_slug { continue; } // self index already in parts
            let Some(root) = roots.get(&key) else {
                eprintln!("[scip_want] skip {repo}: unknown repo slug");
                continue;
            };
            if let Some(p) = crate::scip_setup::ensure_index(root)? {
                parts.push(p);
            }
        }
        match parts.len() {
            0 => Ok(None),
            1 => Ok(Some(parts.remove(0))),
            _ => {
                let merged = std::env::temp_dir()
                    .join(format!("dl-scip-want-{}.scip", std::process::id()));
                scip_import::merge_files(&parts, &merged)?;
                Ok(Some(merged))
            }
        }
    }
}

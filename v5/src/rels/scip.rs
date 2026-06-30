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
pub struct ScipKind;

impl RelKind for ScipKind {
    fn rels(&self) -> &'static [&'static str] {
        &["scip_def", "scip_name", "scip_ref", "scip_edge",
          "scip_fn_edge", "scip_callee_type", "scip_local", "scip_impl"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "scip_def".into(), cols: vec![col("symbol", Type::Text), col("file", Type::Path)] },
            RelDecl { name: "scip_name".into(), cols: vec![col("symbol", Type::Text), col("name", Type::Text)] },
            RelDecl { name: "scip_ref".into(), cols: vec![col("file", Type::Path), col("symbol", Type::Text), col("def_file", Type::Path)] },
            RelDecl { name: "scip_edge".into(), cols: vec![col("src", Type::Path), col("dst", Type::Path)] },
            RelDecl { name: "scip_fn_edge".into(), cols: vec![col("caller", Type::Text), col("callee", Type::Text)] },
            RelDecl { name: "scip_callee_type".into(), cols: vec![col("sym", Type::Text), col("type", Type::Text)] },
            RelDecl { name: "scip_local".into(), cols: vec![col("fn", Type::Text), col("name", Type::Text)] },
            RelDecl { name: "scip_impl".into(), cols: vec![col("impl", Type::Text), col("iface", Type::Text)] },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in SCIP relation"
    }
    fn dirty(&self, changed: &HashSet<String>) -> bool {
        changed.contains("index.scip")
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let t = |s: &str| Value::Text(s.to_string());
        let Some(path) = scip_import::index_path(&eng.root) else {
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

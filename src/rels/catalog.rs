//! Self-describing catalog relations: `rel_catalog`, `fn_catalog`, `op_catalog`.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::{all_builtin_decls, fn_docs, op_docs, Engine};

use super::{col, RelKind};

// --- catalog (self-describing) -----------------------------------------------

/// `rel_catalog(name, group, cols, doc)` + `fn_catalog(name, arity, group, doc)`
/// — the engine describing its own built-in relations and scalar functions, from
/// `all_builtin_decls` (each decl carries its group + doc) / `fn_docs`. Static
/// (no git/file input), so `refresh` always re-emits and reports changed; cheap
/// (bounded by the built-in count).
pub struct CatalogKind;

impl RelKind for CatalogKind {
    fn rels(&self) -> &'static [&'static str] {
        &["rel_catalog", "fn_catalog", "op_catalog", "rel_col"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "rel_catalog".into(), cols: vec![
                col("name", Type::Text), col("group", Type::Text),
                col("cols", Type::Text), col("doc", Type::Text)], group: "meta",
                doc: "this table: every built-in relation with its group, columns, and one-line doc", ..Default::default() },
            RelDecl { name: "rel_col".into(), cols: vec![
                col("rel", Type::Text), col("pos", Type::Int), col("col", Type::Text),
                col("ty", Type::Text), col("variants", Type::Text)], group: "meta",
                doc: "one row per built-in relation column: (rel, 0-based pos, col name, type keyword, variants); variants is the JSON array of allowed values for an enum-vocabulary column (e.g. type_edge.kind), empty for an open column — query it instead of guessing a kind literal", ..Default::default() },
            RelDecl { name: "fn_catalog".into(), cols: vec![
                col("name", Type::Text), col("arity", Type::Int),
                col("group", Type::Text), col("doc", Type::Text)], group: "meta",
                doc: "every scalar function callable in a head or comparison with its arity, group, and one-line doc; sourced from fn_docs", ..Default::default() },
            RelDecl { name: "op_catalog".into(), cols: vec![
                col("op", Type::Text), col("kind", Type::Text),
                col("syntax", Type::Text), col("doc", Type::Text)], group: "meta",
                doc: "every body/sink op (source ops, derived constructs, sinks) with its syntax sketch and one-line semantics; sourced from op_docs", ..Default::default() },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in self-describing relation catalog (rel_catalog / fn_catalog / op_catalog / rel_col)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let rows: Vec<Vec<Value>> = all_builtin_decls().iter().map(|d| {
            let cols = format!("({})",
                d.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", "));
            vec![Value::Text(d.name.clone()), Value::Text(d.group.to_string()),
                 Value::Text(cols), Value::Text(d.doc.to_string())]
        }).collect();
        eng.refresh_rel("rel_catalog", &["name", "group", "cols", "doc"], &rows)?;

        // Per-column rows, with the enum vocabulary for a branded column as a
        // JSON array ("" for an open column). Collect-then-flush: one batched
        // refresh for the whole builtin set.
        let ty_tag = |t: Type| match t {
            Type::Text => "text", Type::Int => "int", Type::Path => "path",
            Type::File => "file", Type::Dir => "dir", Type::Repo => "repo", Type::Rev => "rev",
        };
        let col_rows: Vec<Vec<Value>> = all_builtin_decls().iter().flat_map(|d| {
            let rel = d.name.clone();
            d.cols.iter().enumerate().map(move |(pos, c)| {
                let variants = c.brand.as_deref()
                    .and_then(crate::engine::builtin_enum_variants)
                    .map(|vs| serde_json::to_string(vs).unwrap_or_default())
                    .unwrap_or_default();
                vec![Value::Text(rel.clone()), Value::Int(pos as i64),
                     Value::Text(c.name.clone()), Value::Text(ty_tag(c.ty).to_string()),
                     Value::Text(variants)]
            }).collect::<Vec<_>>()
        }).collect();
        eng.refresh_rel("rel_col", &["rel", "pos", "col", "ty", "variants"], &col_rows)?;

        let fn_rows: Vec<Vec<Value>> = fn_docs().iter().map(|(n, a, g, d)| {
            vec![Value::Text(n.to_string()), Value::Int(*a as i64),
                 Value::Text(g.to_string()), Value::Text(d.to_string())]
        }).collect();
        eng.refresh_rel("fn_catalog", &["name", "arity", "group", "doc"], &fn_rows)?;

        let op_rows: Vec<Vec<Value>> = op_docs().iter().map(|(op, kind, syn, d)| {
            vec![Value::Text(op.to_string()), Value::Text(kind.to_string()),
                 Value::Text(syn.to_string()), Value::Text(d.to_string())]
        }).collect();
        eng.refresh_rel("op_catalog", &["op", "kind", "syntax", "doc"], &op_rows)?;
        Ok(true)
    }
}

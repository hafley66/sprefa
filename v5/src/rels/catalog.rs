//! Self-describing catalog relations: `rel_catalog`, `fn_catalog`, `op_catalog`.

use anyhow::Result;
use std::collections::HashMap;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::{all_builtin_decls, builtin_rel_docs, fn_docs, op_docs, Engine};

use super::{col, RelKind};

// --- catalog (self-describing) -----------------------------------------------

/// `rel_catalog(name, group, cols, doc)` + `fn_catalog(name, arity, group, doc)`
/// — the engine describing its own built-in relations and scalar functions, from
/// `all_builtin_decls` / `builtin_rel_docs` / `fn_docs`. Static (no git/file
/// input), so `refresh` always re-emits and reports changed; cheap (bounded by
/// the built-in count).
pub struct CatalogKind;

impl RelKind for CatalogKind {
    fn rels(&self) -> &'static [&'static str] {
        &["rel_catalog", "fn_catalog", "op_catalog"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "rel_catalog".into(), cols: vec![
                col("name", Type::Text), col("group", Type::Text),
                col("cols", Type::Text), col("doc", Type::Text)] },
            RelDecl { name: "fn_catalog".into(), cols: vec![
                col("name", Type::Text), col("arity", Type::Int),
                col("group", Type::Text), col("doc", Type::Text)] },
            RelDecl { name: "op_catalog".into(), cols: vec![
                col("op", Type::Text), col("kind", Type::Text),
                col("syntax", Type::Text), col("doc", Type::Text)] },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in self-describing relation catalog (rel_catalog / fn_catalog / op_catalog)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let docs: HashMap<&str, (&str, &str)> =
            builtin_rel_docs().iter().map(|(n, g, s)| (*n, (*g, *s))).collect();
        let rows: Vec<Vec<Value>> = all_builtin_decls().iter().map(|d| {
            let cols = format!("({})",
                d.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", "));
            let (group, summary) = docs.get(d.name.as_str()).copied().unwrap_or(("", ""));
            vec![Value::Text(d.name.clone()), Value::Text(group.to_string()),
                 Value::Text(cols), Value::Text(summary.to_string())]
        }).collect();
        eng.refresh_rel("rel_catalog", &["name", "group", "cols", "doc"], &rows)?;

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

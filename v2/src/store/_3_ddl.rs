//! Per-expr DDL builders plus schema + extract hash helpers.
//!
//! Table name convention matches v1's `rule_tables.rs`:
//! - data table: `{ns}__{name}_data` or `{name}_data`
//! - value view: `{ns}__{name}` or `{name}`
//! - refs view:  `{ns}__{name}_refs` or `{name}_refs`
//!
//! Each capture column is stored as twin columns: `{col}_ref` INTEGER
//! (FK into `refs.id`) and `{col}_str` INTEGER (FK into `strings.id`).
//! Scan-pointer captures add `{col}_repo_id` / `{col}_rev_id` /
//! `{col}_file_id` for cross-repo joins.

use std::sync::Arc;

use crate::_0_types::CursorExpr;

use super::_1_trait::ExprTableSpec;

pub fn data_table_name(spec: &ExprTableSpec) -> String {
    match &spec.namespace {
        Some(ns) => format!("{ns}__{}_data", spec.expr_name),
        None     => format!("{}_data", spec.expr_name),
    }
}

pub fn view_name(spec: &ExprTableSpec) -> String {
    match &spec.namespace {
        Some(ns) => format!("{ns}__{}", spec.expr_name),
        None     => spec.expr_name.to_string(),
    }
}

pub fn refs_view_name(spec: &ExprTableSpec) -> String {
    match &spec.namespace {
        Some(ns) => format!("{ns}__{}_refs", spec.expr_name),
        None     => format!("{}_refs", spec.expr_name),
    }
}

pub fn build_data_table_ddl(spec: &ExprTableSpec) -> String {
    let mut cols = vec!["id INTEGER PRIMARY KEY".to_string()];
    for c in &spec.captures {
        cols.push(format!("\"{}_ref\" INTEGER", c.name));
        cols.push(format!("\"{}_str\" INTEGER", c.name));
        if c.scan_pointer.is_some() {
            cols.push(format!("\"{}_repo_id\" INTEGER", c.name));
            cols.push(format!("\"{}_rev_id\" TEXT",    c.name));
            cols.push(format!("\"{}_file_id\" INTEGER",c.name));
        }
    }
    cols.push("repo_id INTEGER".to_string());
    cols.push("file_id INTEGER".to_string());
    cols.push("rev TEXT".to_string());
    format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (\n  {}\n)",
        data_table_name(spec),
        cols.join(",\n  ")
    )
}

pub fn build_view_ddl(spec: &ExprTableSpec) -> String {
    let table = data_table_name(spec);
    let view  = view_name(spec);
    let mut selects = vec!["t.id".to_string()];
    let mut joins   = Vec::<String>::new();
    for (i, c) in spec.captures.iter().enumerate() {
        let alias = format!("s{i}");
        selects.push(format!("{alias}.value AS \"{}\"", c.name));
        selects.push(format!("{alias}.norm  AS \"{}_norm\"", c.name));
        joins.push(format!(
            "LEFT JOIN strings {alias} ON {alias}.id = t.\"{}_str\"",
            c.name
        ));
    }
    selects.push("t.repo_id AS repo_id".to_string());
    selects.push("t.file_id AS file_id".to_string());
    selects.push("t.rev     AS rev".to_string());
    format!(
        "CREATE VIEW IF NOT EXISTS \"{view}\" AS SELECT {sel} FROM \"{table}\" t {joins}",
        sel   = selects.join(", "),
        joins = joins.join(" ")
    )
}

pub fn build_refs_view_ddl(spec: &ExprTableSpec) -> String {
    let table = data_table_name(spec);
    let view  = refs_view_name(spec);
    let mut selects = vec!["t.id".to_string()];
    let mut joins   = Vec::<String>::new();
    for (i, c) in spec.captures.iter().enumerate() {
        let alias = format!("r{i}");
        selects.push(format!("{alias}.span_start AS \"{}_start\"", c.name));
        selects.push(format!("{alias}.span_end   AS \"{}_end\"",   c.name));
        selects.push(format!("{alias}.file_id    AS \"{}_file\"",  c.name));
        joins.push(format!(
            "LEFT JOIN refs {alias} ON {alias}.id = t.\"{}_ref\"",
            c.name
        ));
    }
    format!(
        "CREATE VIEW IF NOT EXISTS \"{view}\" AS SELECT {sel} FROM \"{table}\" t {joins}",
        sel   = selects.join(", "),
        joins = joins.join(" ")
    )
}

/// Schema hash: expr name + per-capture (name + scan_pointer annotation).
/// Drift on this triggers DROP + rebuild of the data table.
pub fn schema_hash_of(spec: &ExprTableSpec) -> Arc<str> {
    let mut h = blake3::Hasher::new();
    h.update(spec.expr_name.as_bytes());
    h.update(b"\0");
    if let Some(ns) = &spec.namespace {
        h.update(ns.as_bytes());
    }
    h.update(b"\0");
    for c in &spec.captures {
        h.update(c.name.as_bytes());
        h.update(b":");
        h.update(c.scan_pointer.as_deref().unwrap_or("").as_bytes());
        h.update(b"\0");
    }
    Arc::from(h.finalize().to_hex().to_string())
}

/// Extract hash: every op's parse_site byte-range source text. Drift here
/// triggers CLEAR (truncate rows; keep table) so the same schema re-runs
/// with new extraction logic.
pub fn extract_hash_of(expr: &CursorExpr) -> Arc<str> {
    let mut h = blake3::Hasher::new();
    for_each_op_parse_site(&expr.pipeline, &mut |_site| {
        // ParseSite carries file + path + byte_range. Source bytes are
        // not kept on ParseSite alone; fall back to hashing the path +
        // byte_range tuple which is stable across re-parses of unchanged
        // text. When the source actually changes, the byte_range bounds
        // change with the edit, which is enough to trip the drift check.
        h.update(_site.file.to_string_lossy().as_bytes());
        h.update(b"\0");
        h.update(&_site.byte_range.start.to_le_bytes());
        h.update(&_site.byte_range.end.to_le_bytes());
    });
    Arc::from(h.finalize().to_hex().to_string())
}

fn for_each_op_parse_site<F>(pipeline: &crate::_5_op::Pipeline, f: &mut F)
where
    F: FnMut(&crate::_0_types::ParseSite),
{
    use crate::_5_op::Pipeline;
    match pipeline {
        Pipeline::Op(lop) => {
            f(lop.op.parse_site());
        }
        Pipeline::Seq(children) => {
            for child in children {
                for_each_op_parse_site(child, f);
            }
        }
        Pipeline::Fork(arms) => {
            for arm in arms {
                for_each_op_parse_site(&arm.pipeline, f);
            }
        }
        Pipeline::Switch { arms, .. } => {
            for (_, arm) in arms {
                for_each_op_parse_site(arm, f);
            }
        }
    }
}

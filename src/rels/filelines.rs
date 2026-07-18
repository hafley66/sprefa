//! Per-file line-count relation: `file_lines(repo, path, rev, line_count)`.
//!
//! Rides the `_file` meta table's `lines` column, populated at scan time by
//! `enumerate_with_hash` (src/engine/mod.rs): the WORK arm counts lines from
//! the same bytes it already reads to hash, and reuses the stored count on the
//! mtime+size fast path. Git-rev rows never get a count (counting blob
//! contents would spawn a read per blob) and stay at the sentinel -1, so this
//! refresh only ever projects rows with `lines >= 0`.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::txt_tbl;

use super::{col, RelKind};

/// `file_lines(repo, path, rev, line_count)`: one row per scanned WORK file
/// with a known line count. Self-diffs against the projected rel each
/// refresh, so a warm no-change tick writes nothing.
pub struct FileLinesKind;

impl RelKind for FileLinesKind {
    fn rels(&self) -> &'static [&'static str] {
        &["file_lines"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "file_lines".into(),
            cols: vec![
                col("repo", Type::Text),
                col("path", Type::Path),
                col("rev", Type::Text),
                col("line_count", Type::Int),
            ],
            group: "file",
            doc: "line count per scanned file, from the corpus walk's byte count (no lossy read); line_count = -1 when unknown (git revs — counting blob contents would spawn a read per blob, so only WORK files are counted) and those rows are excluded here",
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in per-file line-count relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut rows: Vec<(String, String, String, i64)> = eng.db.query_rows(
            "_file",
            "SELECT repo, path, rev, lines FROM _file WHERE lines >= 0 ORDER BY repo, path, rev",
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?,
            )),
        )?;
        rows.sort();
        let existing: Vec<(String, String, String, i64)> = eng.db.query_rows(
            "file_lines",
            &format!(
                "SELECT \"repo\", \"path\", \"rev\", \"line_count\" FROM {} ORDER BY \"repo\", \"path\", \"rev\"",
                txt_tbl("file_lines")
            ),
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?,
            )),
        )?;
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(repo, path, rev, n)| vec![
                Value::Text(repo), Value::Text(path), Value::Text(rev), Value::Int(n),
            ]).collect();
        eng.refresh_rel("file_lines", &["repo", "path", "rev", "line_count"], &out)?;
        Ok(true)
    }
}

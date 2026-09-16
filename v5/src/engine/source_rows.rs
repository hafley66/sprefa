//! Source-row lifecycle (split out of `reconcile.rs` in the file-budget
//! decomp, 2026-07-18): both sides of the `(repo, path)`-keyed `_prov` map —
//! inserting source facts with their map rows, and retracting every row a set
//! of paths solely provided.

use super::*;

impl Engine {
    pub(crate) fn retract_path(
        &self,
        repo: &str,
        path: &str,
        source_rels: &[String],
    ) -> Result<usize> {
        self.retract_paths(&[(repo, path)], source_rels)
    }

    /// Retract every row sourced only from these `(repo, path)` pairs. Prune
    /// `_prov` for all pairs first, then run the orphan sweep once per relation
    /// (not once per pair): a row survives iff some remaining path still provides
    /// its `__src`. Turns the old O(paths x rels x table) into O(rels x table).
    /// Keying by `(repo, path)` keeps two repos sharing a path from retracting
    /// each other's source rows.
    pub(crate) fn retract_paths(
        &self,
        paths: &[(&str, &str)],
        source_rels: &[String],
    ) -> Result<usize> {
        if paths.is_empty() {
            return Ok(0);
        }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _retract_path(repo TEXT, path TEXT, PRIMARY KEY (repo, path))")?;
        self.db.exec("DELETE FROM _retract_path")?;
        let path_rows: Vec<Vec<Value>> = paths
            .iter()
            .map(|(repo, p)| {
                vec![
                    Value::Text((*repo).to_string()),
                    Value::Text((*p).to_string()),
                ]
            })
            .collect();
        self.db
            .insert_rows("_retract_path", &["repo", "path"], &path_rows)?;
        self.db.exec(
            "DELETE FROM _prov WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)",
        )?;
        // Drop located rows attributed to these (repo, path) pairs; fresh spans
        // re-insert on reparse. Sentinel row has path '' and is never retracted.
        // Keying by (repo, path) keeps two config repos sharing a path from
        // retracting each other's located rows.
        self.db.exec(
            "DELETE FROM _where_bytes WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)",
        )?;
        let mut removed = 0usize;
        for rel in source_rels {
            let rel_lit = rel.replace('\'', "''");
            let sql = format!(
                "DELETE FROM {} WHERE __src NOT IN (SELECT src FROM _prov WHERE rel = '{rel_lit}')",
                tbl(rel),
            );
            removed += self.db.exec(&sql)?;
        }
        Ok(removed)
    }

    pub(crate) fn insert_source_rows(
        &self,
        rel: &str,
        meta: &RelMeta,
        repo: &str,
        path: &str,
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let path_rows: Vec<(String, String, Vec<Value>)> = rows
            .iter()
            .cloned()
            .map(|row| (repo.to_string(), path.to_string(), row))
            .collect();
        self.insert_source_rows_for_paths(rel, meta, &path_rows)
    }

    /// Insert source facts plus their `_prov` map rows. Each input is
    /// `(repo slug, path, row)`; `_prov` records `(rel, repo, path, __src)` so
    /// retraction can prune by `(repo, path)` without cross-repo collision.
    pub(crate) fn insert_source_rows_for_paths(
        &self,
        rel: &str,
        meta: &RelMeta,
        rows: &[(String, String, Vec<Value>)],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        self.insert_spine_strings(rows)?;
        let col_names: Vec<&str> = meta.cols.iter().map(|col| col.name.as_str()).collect();
        let plain_rows: Vec<Vec<Value>> = rows.iter().map(|(_, _, row)| row.clone()).collect();
        let encoded_rows = self.encode_rel_rows(rel, &col_names, &plain_rows)?;
        let mut fact_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        let mut prov_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        for ((repo, path, row), mut encoded) in rows.iter().zip(encoded_rows) {
            let src = row_hash(row);
            encoded.push(Value::Text(src.clone()));
            fact_rows.push(encoded);
            prov_rows.push(vec![
                Value::Text(rel.to_string()),
                Value::Text(repo.to_string()),
                Value::Text(path.to_string()),
                Value::Text(src),
            ]);
        }
        let mut cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
        cols.push("__src".to_string());
        let col_refs: Vec<&str> = cols.iter().map(|c| c.as_str()).collect();
        let table = tbl(rel);
        let inserted = self.db.insert_rows(&table, &col_refs, &fact_rows)?;
        self.db
            .insert_rows("_prov", &["rel", "repo", "path", "src"], &prov_rows)?;
        Ok(inserted)
    }
}

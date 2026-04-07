use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_schema::rule_tables::RuleTableDef;

use crate::store::{FileResult, RuleTableSpec, ScanContext, Store};

/// SQLite's bundled libsqlite3 in sqlx supports up to 32766 bound params.
const STR_CHUNK: usize = 2000;
const FILE_CHUNK: usize = 2000;
const REF_CHUNK: usize = 1000;

/// Persistent storage backed by SQLite.
///
/// Per-rule data tables hold extraction results (one row per event).
/// The refs/strings index stays separate for FTS, refactoring, and LSP provenance.
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Intern a batch of string values, returning value -> string_id.
    async fn intern_strings(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        values: &[String],
    ) -> Result<HashMap<String, i64>> {
        if values.is_empty() {
            return Ok(HashMap::new());
        }

        // INSERT OR IGNORE (dedup at DB level).
        for chunk in values.chunks(STR_CHUNK) {
            let ph = chunk.iter().map(|_| "(?,?,NULL)").collect::<Vec<_>>().join(",");
            let sql = format!("INSERT OR IGNORE INTO strings (value, norm, norm2) VALUES {ph}");
            let mut q = sqlx::query(&sql);
            for v in chunk {
                let norm = v.trim().to_lowercase();
                q = q.bind(v.as_str()).bind(norm);
            }
            q.execute(&mut **tx).await?;
        }

        // Read back IDs.
        let mut map = HashMap::with_capacity(values.len());
        for chunk in values.chunks(STR_CHUNK) {
            let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("SELECT id, value FROM strings WHERE value IN ({ph})");
            let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
            for v in chunk {
                q = q.bind(v.as_str());
            }
            for (id, value) in q.fetch_all(&mut **tx).await? {
                map.insert(value, id);
            }
        }
        Ok(map)
    }

    /// Insert ref rows (physical locations), returning (file_id, string_id, span_start) -> ref_id.
    async fn insert_refs(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        refs: &[(i64, i64, i64, i64, bool, Option<i64>, Option<&str>)],
    ) -> Result<HashMap<(i64, i64, i64), i64>> {
        // Bulk insert.
        for chunk in refs.chunks(REF_CHUNK) {
            let ph = chunk.iter().map(|_| "(?,?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
            let sql = format!(
                "INSERT OR IGNORE INTO refs \
                 (string_id, file_id, span_start, span_end, is_path, parent_key_string_id, node_path) \
                 VALUES {ph}"
            );
            let mut q = sqlx::query(&sql);
            for (str_id, file_id, ss, se, is_path, pk_str_id, node_path) in chunk {
                q = q
                    .bind(str_id)
                    .bind(file_id)
                    .bind(ss)
                    .bind(se)
                    .bind(is_path)
                    .bind(pk_str_id)
                    .bind(*node_path);
            }
            q.execute(&mut **tx).await?;
        }

        // Read back ref IDs for all file_ids in the batch.
        let file_ids: Vec<i64> = refs.iter().map(|r| r.1).collect::<std::collections::HashSet<_>>().into_iter().collect();
        let mut map = HashMap::new();
        for chunk in file_ids.chunks(FILE_CHUNK) {
            let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, file_id, string_id, span_start FROM refs WHERE file_id IN ({ph})"
            );
            let mut q = sqlx::query_as::<_, (i64, i64, i64, i64)>(&sql);
            for fid in chunk {
                q = q.bind(fid);
            }
            for (id, fid, sid, ss) in q.fetch_all(&mut **tx).await? {
                map.insert((fid, sid, ss), id);
            }
        }
        Ok(map)
    }

    /// Ensure a file row exists, return file_id.
    async fn ensure_file(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        repo_id: i64,
        rel_path: &str,
        content_hash: &str,
        stem: Option<&str>,
        ext: Option<&str>,
    ) -> Result<i64> {
        let dir = std::path::Path::new(rel_path)
            .parent()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty());

        sqlx::query(
            "INSERT INTO files (repo_id, path, content_hash, stem, ext, dir) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(repo_id, path, content_hash) DO NOTHING",
        )
        .bind(repo_id)
        .bind(rel_path)
        .bind(content_hash)
        .bind(stem)
        .bind(ext)
        .bind(dir)
        .execute(&mut **tx)
        .await?;

        let file_id: i64 =
            sqlx::query_scalar("SELECT id FROM files WHERE repo_id = ? AND path = ?")
                .bind(repo_id)
                .bind(rel_path)
                .fetch_one(&mut **tx)
                .await?;
        Ok(file_id)
    }
}

impl Store for SqliteStore {
    async fn ensure_repo(&self, name: &str, root_path: &str) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO repos (name, root_path) VALUES (?, ?) \
             ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path \
             RETURNING id",
        )
        .bind(name)
        .bind(root_path)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn ensure_rev(&self, repo: &str, rev: &str) -> Result<()> {
        let repo_id: i64 = sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(repo)
            .fetch_one(&self.pool)
            .await?;

        let is_wt = crate::is_wt_rev(rev);
        sqlx::query(
            "INSERT INTO repo_revs (repo_id, rev, is_working_tree) VALUES (?, ?, ?) \
             ON CONFLICT(repo_id, rev) DO UPDATE SET is_working_tree = excluded.is_working_tree",
        )
        .bind(repo_id)
        .bind(rev)
        .bind(is_wt)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn flush_batch(
        &self,
        repo: &str,
        rev: &str,
        files: &[FileResult],
    ) -> Result<usize> {
        if files.is_empty() {
            return Ok(0);
        }

        let repo_id = self.ensure_repo(repo, "").await?;
        self.ensure_rev(repo, rev).await?;

        let mut tx = self.pool.begin().await?;

        // Collect all unique string values across all files/captures.
        let mut all_values: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for file in files {
            for (_, rows) in &file.rule_rows {
                for row in rows {
                    for cap in &row.captures {
                        if seen.insert(cap.value.clone()) {
                            all_values.push(cap.value.clone());
                        }
                        if let Some(pk) = &cap.parent_key {
                            if seen.insert(pk.clone()) {
                                all_values.push(pk.clone());
                            }
                        }
                    }
                }
            }
        }

        let string_ids = self.intern_strings(&mut tx, &all_values).await?;

        // Process each file.
        let mut total_rows = 0usize;
        for file in files {
            let file_id = self.ensure_file(
                &mut tx,
                repo_id,
                &file.rel_path,
                &file.content_hash,
                file.stem.as_deref(),
                file.ext.as_deref(),
            )
            .await?;

            // Ensure rev_files junction.
            sqlx::query("INSERT OR IGNORE INTO rev_files (repo_id, rev, file_id) VALUES (?, ?, ?)")
                .bind(repo_id)
                .bind(rev)
                .bind(file_id)
                .execute(&mut *tx)
                .await?;

            // Build ref rows for the string index.
            let mut ref_rows: Vec<(i64, i64, i64, i64, bool, Option<i64>, Option<&str>)> =
                Vec::new();
            for (_, rows) in &file.rule_rows {
                for row in rows {
                    for cap in &row.captures {
                        let str_id = match string_ids.get(&cap.value) {
                            Some(&id) => id,
                            None => continue,
                        };
                        let pk_str_id = cap
                            .parent_key
                            .as_ref()
                            .and_then(|pk| string_ids.get(pk).copied());
                        ref_rows.push((
                            str_id,
                            file_id,
                            cap.span_start as i64,
                            cap.span_end as i64,
                            cap.is_path,
                            pk_str_id,
                            cap.node_path.as_deref(),
                        ));
                    }
                }
            }

            let ref_ids = self.insert_refs(&mut tx, &ref_rows).await?;

            // Insert per-rule table rows.
            for (rule_name, rows) in &file.rule_rows {
                if rows.is_empty() {
                    continue;
                }

                // Derive column order from first row.
                let col_order: Vec<&str> =
                    rows[0].captures.iter().map(|c| c.column.as_str()).collect();

                let mut col_names: Vec<String> = col_order
                    .iter()
                    .flat_map(|k| [format!("{k}_ref"), format!("{k}_str")])
                    .collect();
                col_names.extend(["repo_id".into(), "file_id".into(), "rev".into()]);

                let params_per_row = col_names.len();
                let ph = format!(
                    "({})",
                    (0..params_per_row)
                        .map(|_| "?")
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let table_name = format!("{rule_name}_data");
                let col_list = col_names.join(", ");
                let chunk_size = (32000 / params_per_row).max(1);

                for chunk in rows.chunks(chunk_size) {
                    let phs = chunk.iter().map(|_| ph.as_str()).collect::<Vec<_>>().join(",");
                    let sql =
                        format!("INSERT INTO \"{table_name}\" ({col_list}) VALUES {phs}");
                    let mut q = sqlx::query(&sql);

                    for row in chunk {
                        for col_name in &col_order {
                            if let Some(cap) =
                                row.captures.iter().find(|c| c.column.as_str() == *col_name)
                            {
                                let str_id = string_ids.get(&cap.value).copied();
                                let ref_id = str_id.and_then(|sid| {
                                    ref_ids
                                        .get(&(file_id, sid, cap.span_start as i64))
                                        .copied()
                                });
                                q = q.bind(ref_id).bind(str_id);
                            } else {
                                q = q.bind(Option::<i64>::None).bind(Option::<i64>::None);
                            }
                        }
                        q = q.bind(repo_id).bind(file_id).bind(rev);
                    }

                    match q.execute(&mut *tx).await {
                        Ok(_) => {}
                        Err(e) if e.to_string().contains("no such table") => {
                            tracing::debug!("rule table {table_name} not found, skipping");
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                total_rows += rows.len();
            }
        }

        tx.commit().await?;
        Ok(total_rows)
    }

    async fn create_rule_tables(&self, tables: &[RuleTableSpec]) -> Result<()> {
        for spec in tables {
            let def = RuleTableDef::from_matches(
                &spec.rule_name,
                &spec
                    .columns
                    .iter()
                    .map(|(name, scan)| (name.clone(), scan.clone()))
                    .collect::<Vec<_>>(),
            );

            sqlx::query(&def.create_table_sql())
                .execute(&self.pool)
                .await?;

            // Views reference the data table -- drop and recreate to pick up schema changes.
            let _ = sqlx::query(&format!("DROP VIEW IF EXISTS \"{}\"", spec.rule_name))
                .execute(&self.pool)
                .await;
            let _ = sqlx::query(&format!("DROP VIEW IF EXISTS \"{}_refs\"", spec.rule_name))
                .execute(&self.pool)
                .await;

            sqlx::query(&def.create_view_sql())
                .execute(&self.pool)
                .await?;
            sqlx::query(&def.create_refs_view_sql())
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn unscanned_repos(&self, table: &str, column: &str) -> Result<Vec<String>> {
        // Query the per-rule table for repo values not yet in repos table.
        let sql = format!(
            "SELECT DISTINCT s.value FROM \"{table}_data\" t \
             JOIN strings s ON t.{column}_str = s.id \
             WHERE s.value NOT IN (SELECT name FROM repos)"
        );
        let rows = sqlx::query_scalar::<_, String>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn unscanned_revs(
        &self,
        table: &str,
        column: &str,
    ) -> Result<Vec<(String, String)>> {
        // Find (repo_name, rev_value) pairs where the rev hasn't been scanned.
        // This requires knowing which column is the repo column. For now,
        // scan targets pair repo+rev columns from the same rule.
        // This is a simplified version -- Step 4 will refine the query.
        let sql = format!(
            "SELECT DISTINCT s.value, '' FROM \"{table}_data\" t \
             JOIN strings s ON t.{column}_str = s.id \
             LEFT JOIN repo_revs rr ON rr.rev = s.value \
             WHERE rr.rev IS NULL"
        );
        let rows = sqlx::query_as::<_, (String, String)>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn unscanned_rev_pairs(
        &self,
        table: &str,
        repo_column: &str,
        rev_column: &str,
    ) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT DISTINCT repo_s.value, rev_s.value \
             FROM \"{table}_data\" t \
             JOIN strings repo_s ON t.{repo_column}_str = repo_s.id \
             JOIN strings rev_s ON t.{rev_column}_str = rev_s.id \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM repo_revs rr \
                 JOIN repos r ON rr.repo_id = r.id \
                 WHERE r.name = repo_s.value AND rr.rev = rev_s.value \
             )"
        );
        let rows = sqlx::query_as::<_, (String, String)>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn delete_files(
        &self,
        repo: &str,
        rev: &str,
        paths: &[String],
    ) -> Result<usize> {
        // Delegate to existing implementation for now.
        crate::flush::delete_rev_files_by_paths(&self.pool, repo, rev, paths).await
    }

    async fn rename_files(
        &self,
        repo: &str,
        renames: &[(String, String)],
    ) -> Result<usize> {
        crate::flush::rename_file_paths(&self.pool, repo, renames).await
    }

    async fn has_stale_scanner_hash(&self, repo: &str, current_hash: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM files f
             JOIN repos r ON f.repo_id = r.id
             WHERE r.name = ? AND f.scanner_hash IS NOT NULL AND f.scanner_hash != ?",
        )
        .bind(repo)
        .bind(current_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn load_scan_context(&self, repo: &str, scanner_hash: &str) -> Result<ScanContext> {
        let repo_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
                .bind(repo)
                .fetch_optional(&self.pool)
                .await?;

        let Some(repo_id) = repo_id else {
            return Ok(ScanContext {
                skip_set: std::collections::HashSet::new(),
            });
        };

        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT path, content_hash FROM files WHERE repo_id = ? AND scanner_hash = ?",
        )
        .bind(repo_id)
        .bind(scanner_hash)
        .fetch_all(&self.pool)
        .await?;

        Ok(ScanContext {
            skip_set: rows.into_iter().collect(),
        })
    }

    fn sqlite_pool(&self) -> Option<&sqlx::SqlitePool> {
        Some(&self.pool)
    }
}

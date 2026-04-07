use std::collections::HashMap;

use anyhow::Result;
use sqlx::{Column, Row, SqlitePool};

use sprefa_schema::rule_tables::RuleTableDef;
use sprefa_sprf::hash::RuleHashes;

use crate::store::{FileResult, RuleChangeKind, RuleTableSpec, ScanContext, Store};

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
        scanner_hash: &str,
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

        // --- Bulk file upsert (kills N+1) ---
        // Collect file metadata, then INSERT...ON CONFLICT in chunks.
        // Read back (path -> file_id) with a single SELECT per chunk.
        struct FileMeta<'a> {
            rel_path: &'a str,
            content_hash: &'a str,
            stem: Option<&'a str>,
            ext: Option<&'a str>,
            dir: Option<String>,
        }
        let file_metas: Vec<FileMeta> = files
            .iter()
            .map(|f| {
                let dir = std::path::Path::new(&f.rel_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                FileMeta {
                    rel_path: &f.rel_path,
                    content_hash: &f.content_hash,
                    stem: f.stem.as_deref(),
                    ext: f.ext.as_deref(),
                    dir,
                }
            })
            .collect();

        let mut file_ids: HashMap<String, i64> = HashMap::with_capacity(files.len());

        for chunk in file_metas.chunks(FILE_CHUNK) {
            // 7 params per row: repo_id, path, content_hash, stem, ext, dir, scanner_hash
            let ph = chunk.iter().map(|_| "(?,?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
            let sql = format!(
                "INSERT INTO files (repo_id, path, content_hash, stem, ext, dir, scanner_hash) \
                 VALUES {ph} \
                 ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET \
                   scanner_hash = excluded.scanner_hash"
            );
            let mut q = sqlx::query(&sql);
            for fm in chunk {
                q = q
                    .bind(repo_id)
                    .bind(fm.rel_path)
                    .bind(fm.content_hash)
                    .bind(fm.stem)
                    .bind(fm.ext)
                    .bind(fm.dir.as_deref())
                    .bind(scanner_hash);
            }
            q.execute(&mut *tx).await?;

            // Read back IDs (same pattern as intern_strings).
            let sel_ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sel_sql = format!(
                "SELECT id, path FROM files WHERE repo_id = ? AND path IN ({sel_ph})"
            );
            let mut sq = sqlx::query_as::<_, (i64, String)>(&sel_sql);
            sq = sq.bind(repo_id);
            for fm in chunk {
                sq = sq.bind(fm.rel_path);
            }
            for (id, path) in sq.fetch_all(&mut *tx).await? {
                file_ids.insert(path, id);
            }
        }

        // --- Bulk rev_files insert ---
        let rev_file_pairs: Vec<i64> = files
            .iter()
            .filter_map(|f| file_ids.get(&f.rel_path).copied())
            .collect();

        for chunk in rev_file_pairs.chunks(FILE_CHUNK) {
            let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
            let sql = format!("INSERT OR IGNORE INTO rev_files (repo_id, rev, file_id) VALUES {ph}");
            let mut q = sqlx::query(&sql);
            for fid in chunk {
                q = q.bind(repo_id).bind(rev).bind(fid);
            }
            q.execute(&mut *tx).await?;
        }

        // --- Per-file refs + per-rule table rows ---
        let mut total_rows = 0usize;
        for file in files {
            let file_id = match file_ids.get(&file.rel_path) {
                Some(&id) => id,
                None => continue,
            };

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

    async fn create_rule_tables(
        &self,
        tables: &[RuleTableSpec],
        hashes: Option<&std::collections::HashMap<String, RuleHashes>>,
    ) -> Result<()> {
        use crate::store::RuleChangeKind;

        for spec in tables {
            // Check if we have hash info for this rule
            let change_kind = if let Some(hash_map) = hashes {
                if let Some(rule_hashes) = hash_map.get(&spec.rule_name) {
                    self.check_rule_hashes(
                        &spec.rule_name,
                        &rule_hashes.schema_hash,
                        &rule_hashes.extract_hash,
                    )
                    .await
                    .ok()
                    .flatten()
                } else {
                    None
                }
            } else {
                None
            };

            // Decide action based on change kind
            let need_drop = matches!(change_kind, Some(RuleChangeKind::SchemaChanged) | None);
            let need_delete = matches!(change_kind, Some(RuleChangeKind::ExtractChanged));

            let def = RuleTableDef::from_matches(
                &spec.rule_name,
                &spec
                    .columns
                    .iter()
                    .map(|(name, scan)| (name.clone(), scan.clone()))
                    .collect::<Vec<_>>(),
            );

            // DROP table if schema changed or never created
            if need_drop {
                let table_name = def.data_table_name();
                let _ = sqlx::query(&format!("DROP TABLE IF EXISTS \"{}\"", table_name))
                    .execute(&self.pool)
                    .await;
                
                sqlx::query(&def.create_table_sql())
                    .execute(&self.pool)
                    .await?;
            }

            // DELETE all rows if extract changed (but keep table)
            if need_delete {
                let table_name = def.data_table_name();
                sqlx::query(&format!("DELETE FROM \"{}\"", table_name))
                    .execute(&self.pool)
                    .await?;
                tracing::info!("rule '{}': extract changed, deleted all rows", spec.rule_name);
            }

            // Views: drop and recreate to pick up any schema changes
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

            // Update sprf_meta if we have hashes
            if let Some(hash_map) = hashes {
                if let Some(rule_hashes) = hash_map.get(&spec.rule_name) {
                    let _ = self.update_rule_hashes(
                        &spec.rule_name,
                        "rules.sprf", // TODO: pass actual source file path
                        &rule_hashes.schema_hash,
                        &rule_hashes.extract_hash,
                    ).await;
                }
            }
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
        // TODO: migrate to Store trait methods
        crate::flush::delete_rev_files_by_paths(&self.pool, repo, rev, paths).await
    }

    async fn rename_files(
        &self,
        repo: &str,
        renames: &[(String, String)],
    ) -> Result<usize> {
        // TODO: migrate to Store trait methods
        crate::flush::rename_file_paths(&self.pool, repo, renames).await
    }

    async fn stale_file_paths(&self, repo: &str, current_hash: &str) -> Result<Vec<String>> {
        let paths: Vec<String> = sqlx::query_scalar(
            "SELECT f.path FROM files f
             JOIN repos r ON f.repo_id = r.id
             WHERE r.name = ? AND f.scanner_hash IS NOT NULL AND f.scanner_hash != ?",
        )
        .bind(repo)
        .bind(current_hash)
        .fetch_all(&self.pool)
        .await?;
        Ok(paths)
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

    async fn check_rule_hashes(
        &self,
        rule_name: &str,
        schema_hash: &str,
        extract_hash: &str,
    ) -> Result<Option<RuleChangeKind>> {
        // Fail soft: if sprf_meta table doesn't exist, treat as "no history"
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT schema_hash, extract_hash FROM sprf_meta WHERE rule_name = ?",
        )
        .bind(rule_name)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        let Some((stored_schema, stored_extract)) = row else {
            // Rule not in sprf_meta - either new rule or table doesn't exist yet
            return Ok(None);
        };

        if stored_schema != schema_hash {
            return Ok(Some(RuleChangeKind::SchemaChanged));
        }
        if stored_extract != extract_hash {
            return Ok(Some(RuleChangeKind::ExtractChanged));
        }
        Ok(Some(RuleChangeKind::Unchanged))
    }

    async fn update_rule_hashes(
        &self,
        rule_name: &str,
        source_file: &str,
        schema_hash: &str,
        extract_hash: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sprf_meta (rule_name, source_file, schema_hash, extract_hash, last_scanned_at)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(rule_name) DO UPDATE SET
                source_file = excluded.source_file,
                schema_hash = excluded.schema_hash,
                extract_hash = excluded.extract_hash,
                last_scanned_at = datetime('now')",
        )
        .bind(rule_name)
        .bind(source_file)
        .bind(schema_hash)
        .bind(extract_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn sqlite_pool(&self) -> Option<&sqlx::SqlitePool> {
        Some(&self.pool)
    }

    async fn run_check(&self, check_name: &str, sql: &str) -> Result<usize> {
        use sqlx::Row;

        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;

        for row in &rows {
            let mut json_map = serde_json::Map::new();
            for col in row.columns() {
                let name = col.name().to_string();
                let value: Option<String> = row.try_get(name.as_str()).ok().flatten();
                json_map.insert(name, serde_json::Value::String(value.unwrap_or_default()));
            }

            let violation_data = serde_json::to_string(&json_map)?;

            sqlx::query(
                "INSERT INTO invariant_violations (check_name, violation_data) VALUES (?, ?)",
            )
            .bind(check_name)
            .bind(&violation_data)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(rows.len())
    }

    async fn list_violations(
        &self,
        check_name: Option<&str>,
    ) -> Result<Vec<crate::store::ViolationEntry>> {
        let sql = if check_name.is_some() {
            "SELECT id, check_name, violation_data, created_at, resolved_at FROM invariant_violations WHERE check_name = ?"
        } else {
            "SELECT id, check_name, violation_data, created_at, resolved_at FROM invariant_violations"
        };

        let mut query = sqlx::query(sql);
        if let Some(name) = check_name {
            query = query.bind(name);
        }

        let rows = query.fetch_all(&self.pool).await?;

        let violations = rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get("id");
                let check_name: String = row.get("check_name");
                let violation_data: String = row.get("violation_data");
                let created_at: String = row.get("created_at");
                let resolved_at: Option<String> = row.get("resolved_at");

                crate::store::ViolationEntry {
                    id,
                    check_name,
                    violation_data,
                    created_at,
                    resolved_at,
                }
            })
            .collect();

        Ok(violations)
    }
}

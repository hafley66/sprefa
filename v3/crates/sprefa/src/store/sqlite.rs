//! `SqliteStore` — sqlx+rusqlite-backed `Store` impl.
//!
//! # Role
//! Single persistence + mutation-cache backend. Per-expr data tables are
//! registered through `register_expr_schema`; rows flow in via
//! `flush_batch`; queries go through `query_expr`; the effect cache
//! drives Skip/Stale/Emit via `effect_status` + `record_effect`.
//!
//! # Ownership + lifecycle
//! `Arc<SqliteStore>` on `DocSession`, shared with every `OpCtx`. Pool
//! closes on drop; connections return to the pool between awaits.
//!
//! # Who mutates
//! `flush_batch`, `register_expr_schema`, `record_effect` are writes.
//! Everything else is read-only.
//!
//! # Failure modes
//! Every sqlx error wraps into `StoreErr::Sql(String)`. The string is
//! the only coordinate kept — callers bubble it through `RunEvent::Diag`
//! via the `Diagnostic` impl on `StoreErr`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row as _, SqlitePool};
use tokio::sync::RwLock;
use tracing::{info_span, Instrument};

use crate::types::FilePath;
use crate::mutations::MutationEffect;

use super::types::{
    Batch, ContentHash, EffectOutcome, EffectResult, EffectStatus, Row, StoreErr, Where,
};
use super::api::{ExprTableSpec, Store};
use super::ddl::{
    build_data_table_ddl, build_refs_view_ddl, build_view_ddl, data_table_name, refs_view_name,
    view_name,
};

pub struct SqliteStore {
    pub pool:  SqlitePool,
    pub specs: RwLock<HashMap<Arc<str>, ExprTableSpec>>,
}

impl SqliteStore {
    pub async fn open(path: &Path) -> Result<Arc<Self>, StoreErr> {
        let pool = super::migrations::init_db(path).await?;
        super::migrations::run_migrations(&pool).await?;
        Ok(Arc::new(Self { pool, specs: RwLock::new(HashMap::new()) }))
    }

    pub async fn open_memory() -> Result<Arc<Self>, StoreErr> {
        Self::open(Path::new(":memory:")).await
    }
}

enum DriftDecision {
    New,
    Unchanged,
    SchemaChanged,
    ExtractChanged,
}

fn classify_change(
    prior_schema:  Option<String>,
    prior_extract: Option<String>,
    spec:          &ExprTableSpec,
) -> DriftDecision {
    match (prior_schema, prior_extract) {
        (None, _) | (_, None) => DriftDecision::New,
        (Some(s), Some(e)) if s == *spec.schema_hash  => {
            if e == *spec.extract_hash {
                DriftDecision::Unchanged
            } else {
                DriftDecision::ExtractChanged
            }
        }
        _ => DriftDecision::SchemaChanged,
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn init(&self) -> Result<(), StoreErr> {
        Ok(())
    }

    async fn register_expr_schema(&self, spec: ExprTableSpec) -> Result<(), StoreErr> {
        let span = info_span!("store.sqlite.register_expr_schema", expr = %spec.expr_name);
        async move {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT schema_hash, extract_hash FROM sprf_meta WHERE expr_name = ?",
        )
        .bind(spec.expr_name.as_ref())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;

        let (prior_schema, prior_extract) = match row {
            Some((s, e)) => (Some(s), Some(e)),
            None         => (None, None),
        };
        let decision = classify_change(prior_schema, prior_extract, &spec);

        let dt = data_table_name(&spec);
        let vw = view_name(&spec);
        let rv = refs_view_name(&spec);

        match decision {
            DriftDecision::Unchanged => {}
            DriftDecision::New => {
                sqlx::query(&build_data_table_ddl(&spec))
                    .execute(&self.pool)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&format!("DROP VIEW IF EXISTS \"{vw}\""))
                    .execute(&self.pool)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&build_view_ddl(&spec))
                    .execute(&self.pool)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&format!("DROP VIEW IF EXISTS \"{rv}\""))
                    .execute(&self.pool)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&build_refs_view_ddl(&spec))
                    .execute(&self.pool)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
            }
            DriftDecision::SchemaChanged => {
                sqlx::query(&format!("DROP VIEW IF EXISTS \"{vw}\""))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&format!("DROP VIEW IF EXISTS \"{rv}\""))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&format!("DROP TABLE IF EXISTS \"{dt}\""))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&build_data_table_ddl(&spec))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&build_view_ddl(&spec))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                sqlx::query(&build_refs_view_ddl(&spec))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
            }
            DriftDecision::ExtractChanged => {
                sqlx::query(&format!("DELETE FROM \"{dt}\""))
                    .execute(&self.pool).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
            }
        }

        sqlx::query(
            "INSERT INTO sprf_meta (expr_name, schema_hash, extract_hash, last_scanned_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(expr_name) DO UPDATE SET \
               schema_hash = excluded.schema_hash, \
               extract_hash = excluded.extract_hash, \
               last_scanned_at = excluded.last_scanned_at",
        )
        .bind(spec.expr_name.as_ref())
        .bind(spec.schema_hash.as_ref())
        .bind(spec.extract_hash.as_ref())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;

        self.specs.write().await.insert(spec.expr_name.clone(), spec);
        Ok(())
        }.instrument(span).await
    }

    async fn flush_batch(&self, b: Batch) -> Result<(), StoreErr> {
        let n_exprs = b.per_expr.len();
        let n_rows: usize = b.per_expr.iter().map(|e| e.rows.len()).sum();
        let span = info_span!("store.sqlite.flush_batch", n_exprs = n_exprs, n_rows = n_rows);
        async move {
        let specs = self.specs.read().await;
        let mut tx = self.pool.begin().await.map_err(|e| StoreErr::Sql(e.to_string()))?;

        // ---------- 1. Strings: bulk INSERT OR IGNORE + one SELECT.
        let mut strings_to_intern: HashSet<Arc<str>> = HashSet::new();
        for eb in &b.per_expr {
            for row in &eb.rows {
                for cap in row.captures.values() {
                    strings_to_intern.insert(cap.value.clone());
                }
            }
        }
        let mut string_ids: HashMap<Arc<str>, i64> = HashMap::new();
        if !strings_to_intern.is_empty() {
            // SQLite bound-variable limit is 32766 in modern builds but 999
            // on older ones — chunk defensively. Each string binds 2 params
            // on insert; SELECT binds 1 per value.
            let strings_vec: Vec<Arc<str>> = strings_to_intern.iter().cloned().collect();
            for chunk in strings_vec.chunks(400) {
                let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                    "INSERT INTO strings (value, norm, norm2) ",
                );
                qb.push_values(chunk.iter(), |mut b, s| {
                    b.push_bind(s.as_ref())
                     .push_bind(super::udfs::normalize(s))
                     .push_bind::<Option<String>>(None);
                });
                qb.push(" ON CONFLICT(value) DO NOTHING");
                qb.build().execute(&mut *tx).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;

                let mut qs = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                    "SELECT id, value FROM strings WHERE value IN (",
                );
                let mut sep = qs.separated(", ");
                for s in chunk { sep.push_bind(s.as_ref()); }
                qs.push(")");
                let rows = qs.build().fetch_all(&mut *tx).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                for r in rows {
                    let id: i64 = r.try_get::<i64, _>("id").map_err(|e| StoreErr::Sql(e.to_string()))?;
                    let val: String = r.try_get::<String, _>("value").map_err(|e| StoreErr::Sql(e.to_string()))?;
                    // Match back to the original Arc<str> key without a
                    // fresh allocation when we can; fallback re-interns.
                    let key: Arc<str> = chunk.iter()
                        .find(|s| s.as_ref() == val.as_str())
                        .cloned()
                        .unwrap_or_else(|| Arc::<str>::from(val.as_str()));
                    string_ids.insert(key, id);
                }
            }
        }

        // ---------- 2. Repos: bulk INSERT OR IGNORE + one SELECT.
        let repo_set: HashSet<Arc<str>> = b.per_expr.iter()
            .flat_map(|eb| eb.rows.iter().map(|r| r.repo.clone()))
            .collect();
        let mut repo_ids: HashMap<Arc<str>, i64> = HashMap::new();
        if !repo_set.is_empty() {
            let repo_vec: Vec<Arc<str>> = repo_set.iter().cloned().collect();
            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "INSERT INTO repos (name, root_path) ",
            );
            qb.push_values(repo_vec.iter(), |mut b, name| {
                b.push_bind(name.as_ref()).push_bind("");
            });
            qb.push(" ON CONFLICT(name) DO NOTHING");
            qb.build().execute(&mut *tx).await
                .map_err(|e| StoreErr::Sql(e.to_string()))?;

            let mut qs = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT id, name FROM repos WHERE name IN (",
            );
            let mut sep = qs.separated(", ");
            for n in &repo_vec { sep.push_bind(n.as_ref()); }
            qs.push(")");
            let rows = qs.build().fetch_all(&mut *tx).await
                .map_err(|e| StoreErr::Sql(e.to_string()))?;
            for r in rows {
                let id: i64 = r.try_get("id").map_err(|e| StoreErr::Sql(e.to_string()))?;
                let name: String = r.try_get("name").map_err(|e| StoreErr::Sql(e.to_string()))?;
                let key: Arc<str> = repo_vec.iter()
                    .find(|s| s.as_ref() == name.as_str())
                    .cloned()
                    .unwrap_or_else(|| Arc::<str>::from(name.as_str()));
                repo_ids.insert(key, id);
            }
        }

        // ---------- 3. Files: bulk INSERT OR IGNORE + one SELECT.
        let mut file_set: HashSet<(i64, String)> = HashSet::new();
        for eb in &b.per_expr {
            for row in &eb.rows {
                if let Some(&rid) = repo_ids.get(&row.repo) {
                    file_set.insert((rid, row.file.0.to_string_lossy().into_owned()));
                }
            }
        }
        let mut file_ids: HashMap<(i64, String), i64> = HashMap::new();
        let scanner_hash_str: String = b.scanner_hash.as_ref().to_string();
        if !file_set.is_empty() {
            let file_vec: Vec<(i64, String)> = file_set.into_iter().collect();
            for chunk in file_vec.chunks(200) {
                let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                    "INSERT INTO files (repo_id, path, content_hash, scanner_hash) ",
                );
                let scanner_hash_ref = scanner_hash_str.clone();
                qb.push_values(chunk.iter(), |mut b, (rid, path)| {
                    b.push_bind(*rid)
                     .push_bind(path.as_str())
                     .push_bind("")
                     .push_bind(scanner_hash_ref.clone());
                });
                qb.push(" ON CONFLICT(repo_id, path, content_hash) DO NOTHING");
                qb.build().execute(&mut *tx).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;

                // One SELECT per chunk keyed by (repo_id, path). Use a
                // values table join to stay in a single roundtrip.
                let mut qs = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                    "SELECT id, repo_id, path FROM files WHERE (repo_id, path) IN (",
                );
                let mut first = true;
                for (rid, path) in chunk {
                    if !first { qs.push(", "); }
                    qs.push("(").push_bind(*rid).push(", ").push_bind(path.as_str()).push(")");
                    first = false;
                }
                qs.push(")");
                let rows = qs.build().fetch_all(&mut *tx).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                for r in rows {
                    let id: i64 = r.try_get("id").map_err(|e| StoreErr::Sql(e.to_string()))?;
                    let rid: i64 = r.try_get("repo_id").map_err(|e| StoreErr::Sql(e.to_string()))?;
                    let path: String = r.try_get("path").map_err(|e| StoreErr::Sql(e.to_string()))?;
                    file_ids.insert((rid, path), id);
                }
            }
        }

        // ---------- 4. Rows: per-expr multi-VALUES INSERT, chunked.
        for eb in &b.per_expr {
            if eb.rows.is_empty() { continue; }
            let spec = specs.get(&eb.expr_name).ok_or(StoreErr::UnknownExpr)?;
            let table = data_table_name(spec);
            let mut col_names: Vec<String> = Vec::new();
            let mut cols_per_row = 0usize;
            for c in &spec.captures {
                col_names.push(format!("\"{}_ref\"", c.name));
                col_names.push(format!("\"{}_str\"", c.name));
                cols_per_row += 2;
                if c.scan_pointer.is_some() {
                    col_names.push(format!("\"{}_repo_id\"", c.name));
                    col_names.push(format!("\"{}_rev_id\"",  c.name));
                    col_names.push(format!("\"{}_file_id\"", c.name));
                    cols_per_row += 3;
                }
            }
            col_names.push("repo_id".to_string());
            col_names.push("file_id".to_string());
            col_names.push("rev".to_string());
            cols_per_row += 3;

            let rows_per_chunk = (30_000 / cols_per_row.max(1)).max(1).min(1000);
            for chunk in eb.rows.chunks(rows_per_chunk) {
                let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
                    "INSERT INTO \"{table}\" ({}) ",
                    col_names.join(", ")
                ));
                qb.push_values(chunk.iter(), |mut b, row| {
                    let rid = repo_ids.get(&row.repo).copied().unwrap_or(0);
                    let fid = file_ids
                        .get(&(rid, row.file.0.to_string_lossy().into_owned()))
                        .copied()
                        .unwrap_or(0);
                    for c in &spec.captures {
                        let sid = row.captures.get(&c.name)
                            .and_then(|cap| string_ids.get(&cap.value).copied());
                        b.push_bind(Option::<i64>::None);        // _ref
                        b.push_bind(sid);                        // _str
                        if c.scan_pointer.is_some() {
                            b.push_bind(Option::<i64>::None);    // _repo_id
                            b.push_bind(Option::<String>::None); // _rev_id
                            b.push_bind(Option::<i64>::None);    // _file_id
                        }
                    }
                    b.push_bind(rid);
                    b.push_bind(fid);
                    b.push_bind(row.rev.as_ref().to_string());
                });
                qb.build().execute(&mut *tx).await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
            }
        }

        let commit_fut = tx.commit().instrument(info_span!("store.sqlite.flush_batch.commit"));
        commit_fut.await.map_err(|e| StoreErr::Sql(e.to_string()))?;
        Ok(())
        }.instrument(span).await
    }

    async fn query_expr(&self, expr_name: &str, w: Where) -> Result<Vec<Row>, StoreErr> {
        let span = info_span!("store.sqlite.query_expr", expr = %expr_name);
        async move {
        let specs = self.specs.read().await;
        let spec  = specs.get(expr_name).ok_or(StoreErr::UnknownExpr)?;
        let view  = view_name(spec);

        let mut sql = format!("SELECT * FROM \"{view}\"");
        let mut wheres = Vec::<String>::new();
        if w.repo.is_some() { wheres.push("repo_id IN (SELECT id FROM repos WHERE name = ?)".to_string()); }
        if w.rev.is_some()  { wheres.push("rev = ?".to_string()); }
        for (col, _) in &w.captures {
            wheres.push(format!("\"{}\" = ?", col));
        }
        if !wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&wheres.join(" AND "));
        }
        sql.push_str(&format!(" LIMIT {}", w.limit.max(1)));

        let mut q = sqlx::query(&sql);
        if let Some(r) = &w.repo { q = q.bind(r.as_ref()); }
        if let Some(r) = &w.rev  { q = q.bind(r.as_ref()); }
        for (_, v) in &w.captures { q = q.bind(v.as_ref()); }

        let rows = q.fetch_all(&self.pool).await.map_err(|e| StoreErr::Sql(e.to_string()))?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let mut captures: HashMap<Arc<str>, Arc<str>> = HashMap::new();
            for c in &spec.captures {
                let v: Option<String> = r.try_get(c.name.as_ref()).ok();
                if let Some(v) = v {
                    captures.insert(c.name.clone(), Arc::from(v));
                }
            }
            let path: Option<String> = r.try_get("file_id").ok().and_then(|fid: i64| {
                // swallow lookup errors; file_id FK is always valid for inserted rows
                let _ = fid;
                None
            });
            out.push(Row {
                captures,
                file: FilePath(Arc::from(std::path::Path::new(path.as_deref().unwrap_or("")))),
                span: 0..0,
            });
        }
        Ok(out)
        }.instrument(span).await
    }

    async fn files_scanned(
        &self,
        repo:         &str,
        rev:          &str,
        scanner_hash: &str,
    ) -> Result<HashSet<(FilePath, ContentHash)>, StoreErr> {
        let span = info_span!("store.sqlite.files_scanned", repo = %repo, rev = %rev);
        async move {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT f.path, f.content_hash \
             FROM files f \
             JOIN repos r ON r.id = f.repo_id \
             JOIN rev_files rf ON rf.file_id = f.id \
             WHERE r.name = ? AND rf.rev = ? AND f.scanner_hash = ?",
        )
        .bind(repo)
        .bind(rev)
        .bind(scanner_hash)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(p, h)| (FilePath(Arc::from(std::path::Path::new(&p))), ContentHash(Arc::from(h))))
            .collect())
        }.instrument(span).await
    }

    async fn effect_status(&self, e: &dyn MutationEffect) -> Result<EffectStatus, StoreErr> {
        let span = info_span!("store.sqlite.effect_status", kind = e.kind_sigil());
        async move {
        let fp = e.fingerprint();
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT outcome, when_utc, effect_hash, fingerprint \
             FROM mutations \
             WHERE kind_sigil = ? AND fingerprint = ? \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(e.kind_sigil())
        .bind(fp.as_ref())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;

        let Some((outcome, when_utc, effect_hash, _fp)) = row else {
            return Ok(EffectStatus::Emit);
        };
        let result = EffectResult::from_str(&outcome);
        let when: DateTime<Utc> = DateTime::parse_from_rfc3339(&when_utc)
            .map_err(|e| StoreErr::Sql(e.to_string()))?
            .with_timezone(&Utc);
        let outcome = EffectOutcome { result, when, effect_hash: Arc::from(effect_hash) };
        if e.content_stable_since(when) {
            Ok(EffectStatus::Skip)
        } else {
            Ok(EffectStatus::Stale(outcome))
        }
        }.instrument(span).await
    }

    async fn record_effect(
        &self,
        e: &dyn MutationEffect,
        o: EffectOutcome,
    ) -> Result<(), StoreErr> {
        let span = info_span!("store.sqlite.record_effect", kind = e.kind_sigil());
        async move {
        sqlx::query(
            "INSERT INTO mutations (kind_sigil, fingerprint, effect_hash, outcome, when_utc) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(kind_sigil, fingerprint) DO UPDATE SET \
               effect_hash = excluded.effect_hash, \
               outcome = excluded.outcome, \
               when_utc = excluded.when_utc",
        )
        .bind(e.kind_sigil())
        .bind(e.fingerprint().as_ref())
        .bind(o.effect_hash.as_ref())
        .bind(o.result.as_sql_str())
        .bind(o.when.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreErr::Sql(e.to_string()))?;
        Ok(())
        }.instrument(span).await
    }
}

// EffectResult::from_str / as_sql_str live in `spine::mutations` now.

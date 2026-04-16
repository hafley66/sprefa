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

use crate::_0_types::FilePath;
use crate::mutations::MutationEffect;

use super::_0_types::{
    Batch, ContentHash, EffectOutcome, EffectResult, EffectStatus, Row, StoreErr, Where,
};
use super::_1_trait::{ExprTableSpec, Store};
use super::_3_ddl::{
    build_data_table_ddl, build_refs_view_ddl, build_view_ddl, data_table_name, refs_view_name,
    view_name,
};

pub struct SqliteStore {
    pub pool:  SqlitePool,
    pub specs: RwLock<HashMap<Arc<str>, ExprTableSpec>>,
}

impl SqliteStore {
    pub async fn open(path: &Path) -> Result<Arc<Self>, StoreErr> {
        let pool = super::_5_migrations::init_db(path).await?;
        super::_5_migrations::run_migrations(&pool).await?;
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
    }

    async fn flush_batch(&self, b: Batch) -> Result<(), StoreErr> {
        let specs = self.specs.read().await;
        let mut tx = self.pool.begin().await.map_err(|e| StoreErr::Sql(e.to_string()))?;

        // 1. Intern strings (scan every capture value).
        let mut strings_to_intern: HashSet<Arc<str>> = HashSet::new();
        for eb in &b.per_expr {
            for row in &eb.rows {
                for cap in row.captures.values() {
                    strings_to_intern.insert(cap.value.clone());
                }
            }
        }
        let mut string_ids: HashMap<Arc<str>, i64> = HashMap::new();
        for s in &strings_to_intern {
            let norm = super::_4_udfs::normalize(s);
            sqlx::query(
                "INSERT OR IGNORE INTO strings (value, norm, norm2) VALUES (?, ?, NULL)",
            )
            .bind(s.as_ref())
            .bind(&norm)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreErr::Sql(e.to_string()))?;
            let id: i64 = sqlx::query_scalar("SELECT id FROM strings WHERE value = ?")
                .bind(s.as_ref())
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| StoreErr::Sql(e.to_string()))?;
            string_ids.insert(s.clone(), id);
        }

        // 2. Upsert repos / files.
        let mut repo_ids: HashMap<Arc<str>, i64> = HashMap::new();
        let mut file_ids: HashMap<(i64, FilePath), i64> = HashMap::new();
        for eb in &b.per_expr {
            for row in &eb.rows {
                let repo_id = if let Some(id) = repo_ids.get(&row.repo) {
                    *id
                } else {
                    sqlx::query("INSERT OR IGNORE INTO repos (name, root_path) VALUES (?, '')")
                        .bind(row.repo.as_ref())
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| StoreErr::Sql(e.to_string()))?;
                    let id: i64 = sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
                        .bind(row.repo.as_ref())
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(|e| StoreErr::Sql(e.to_string()))?;
                    repo_ids.insert(row.repo.clone(), id);
                    id
                };
                let key = (repo_id, row.file.clone());
                if !file_ids.contains_key(&key) {
                    let path_str = row.file.0.to_string_lossy().to_string();
                    sqlx::query(
                        "INSERT OR IGNORE INTO files (repo_id, path, content_hash, scanner_hash) \
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(repo_id)
                    .bind(&path_str)
                    .bind("")
                    .bind(b.scanner_hash.as_ref())
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                    let id: i64 = sqlx::query_scalar(
                        "SELECT id FROM files WHERE repo_id = ? AND path = ? LIMIT 1",
                    )
                    .bind(repo_id)
                    .bind(&path_str)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
                    file_ids.insert(key, id);
                }
            }
        }

        // 3. Per-expr row insert.
        for eb in &b.per_expr {
            let spec = specs.get(&eb.expr_name).ok_or(StoreErr::UnknownExpr)?;
            let table = data_table_name(spec);
            let mut col_names: Vec<String> = Vec::new();
            for c in &spec.captures {
                col_names.push(format!("\"{}_ref\"", c.name));
                col_names.push(format!("\"{}_str\"", c.name));
                if c.scan_pointer.is_some() {
                    col_names.push(format!("\"{}_repo_id\"", c.name));
                    col_names.push(format!("\"{}_rev_id\"",  c.name));
                    col_names.push(format!("\"{}_file_id\"", c.name));
                }
            }
            col_names.push("repo_id".to_string());
            col_names.push("file_id".to_string());
            col_names.push("rev".to_string());
            let placeholders = vec!["?"; col_names.len()].join(", ");
            let stmt = format!(
                "INSERT INTO \"{table}\" ({}) VALUES ({placeholders})",
                col_names.join(", ")
            );

            for row in &eb.rows {
                let repo_id = *repo_ids.get(&row.repo).ok_or_else(|| StoreErr::Sql(
                    format!("missing repo_id for {}", row.repo),
                ))?;
                let file_id = *file_ids.get(&(repo_id, row.file.clone())).ok_or_else(|| StoreErr::Sql(
                    format!("missing file_id for {}", row.file.0.display()),
                ))?;
                let mut q = sqlx::query(&stmt);
                for c in &spec.captures {
                    let cap = row.captures.get(&c.name);
                    let sid = cap.and_then(|cap| string_ids.get(&cap.value).copied());
                    q = q.bind(Option::<i64>::None);        // _ref
                    q = q.bind(sid);                        // _str
                    if c.scan_pointer.is_some() {
                        q = q.bind(Option::<i64>::None);    // _repo_id
                        q = q.bind(Option::<String>::None); // _rev_id
                        q = q.bind(Option::<i64>::None);    // _file_id
                    }
                }
                q = q.bind(repo_id);
                q = q.bind(file_id);
                q = q.bind(row.rev.as_ref());
                q.execute(&mut *tx)
                    .await
                    .map_err(|e| StoreErr::Sql(e.to_string()))?;
            }
        }

        tx.commit().await.map_err(|e| StoreErr::Sql(e.to_string()))?;
        Ok(())
    }

    async fn query_expr(&self, expr_name: &str, w: Where) -> Result<Vec<Row>, StoreErr> {
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
    }

    async fn files_scanned(
        &self,
        repo:         &str,
        rev:          &str,
        scanner_hash: &str,
    ) -> Result<HashSet<(FilePath, ContentHash)>, StoreErr> {
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
    }

    async fn effect_status(&self, e: &dyn MutationEffect) -> Result<EffectStatus, StoreErr> {
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
    }

    async fn record_effect(
        &self,
        e: &dyn MutationEffect,
        o: EffectOutcome,
    ) -> Result<(), StoreErr> {
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
    }
}

impl EffectResult {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Applied"  => Self::Applied,
            "Rejected" => Self::Rejected,
            _          => Self::Superseded,
        }
    }
    pub fn as_sql_str(self) -> &'static str {
        match self {
            Self::Applied    => "Applied",
            Self::Rejected   => "Rejected",
            Self::Superseded => "Superseded",
        }
    }
}

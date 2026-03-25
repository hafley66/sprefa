use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use memmap2::Mmap;
use rayon::prelude::*;
use sqlx::SqlitePool;
use xxhash_rust::xxh3::xxh3_128;

use sprefa_config::{CompiledFilter, NormalizeConfig, RepoConfig};
use sprefa_extract::{Extractor, RawRef};

use crate::{
    files::list_files,
    normalize::{normalize, normalize2},
};

pub struct Scanner {
    pub extractors: Vec<Box<dyn Extractor>>,
    pub db: SqlitePool,
    pub normalize_config: Option<NormalizeConfig>,
}

pub struct ScanResult {
    pub repo: String,
    pub branch: String,
    pub files_scanned: usize,
    pub refs_inserted: usize,
}

/// One extracted file's worth of data, produced in the rayon phase.
struct ExtractedFile {
    rel_path: String,
    content_hash: String,
    stem: Option<String>,
    ext: Option<String>,
    refs: Vec<RawRef>,
}

impl Scanner {
    fn extractor_for(&self, ext: &str) -> Option<&dyn Extractor> {
        self.extractors
            .iter()
            .find(|e| e.extensions().contains(&ext))
            .map(|e| e.as_ref())
    }

    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let repo_path = Path::new(&config.path);

        // Resolve filter for this branch
        let filter_config = sprefa_config::resolve_filter(None, config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let files = list_files(repo_path, compiled_filter.as_ref())?;
        tracing::info!("{}/{}: {} files", config.name, branch, files.len());

        // Parallel: mmap + extract. Collects into Vec preserving order.
        let extracted: Vec<ExtractedFile> = files
            .par_iter()
            .filter_map(|abs_path| {
                let rel = abs_path.strip_prefix(repo_path).ok()?.to_str()?;
                let ext = abs_path.extension().and_then(|e| e.to_str());

                let extractor = ext.and_then(|e| self.extractor_for(e))?;

                let file = std::fs::File::open(abs_path).ok()?;
                // Safety: we don't mutate the file while the mmap is alive.
                // The file is read-only during the scan.
                let mmap = unsafe { Mmap::map(&file).ok()? };

                let hash = format!("{:x}", xxh3_128(&mmap));
                let refs = extractor.extract(&mmap, rel);

                if refs.is_empty() {
                    return None;
                }

                Some(ExtractedFile {
                    rel_path: rel.to_string(),
                    content_hash: hash,
                    stem: abs_path.file_stem().and_then(|s| s.to_str()).map(String::from),
                    ext: ext.map(String::from),
                    refs,
                })
            })
            .collect();

        // Single-threaded DB write in a transaction
        let refs_inserted = self
            .flush(config, branch, extracted)
            .await?;

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned: files.len(),
            refs_inserted,
        })
    }

    async fn flush(
        &self,
        config: &RepoConfig,
        branch: &str,
        files: Vec<ExtractedFile>,
    ) -> Result<usize> {
        // Upsert repo + branch outside the main transaction (idempotent metadata)
        let repo_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO repos (name, root_path) VALUES (?, ?)
             ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path
             RETURNING id",
        )
        .bind(&config.name)
        .bind(&config.path)
        .fetch_one(&self.db)
        .await?;

        sqlx::query(
            "INSERT INTO repo_branches (repo_id, branch) VALUES (?, ?)
             ON CONFLICT(repo_id, branch) DO NOTHING",
        )
        .bind(repo_id)
        .bind(branch)
        .execute(&self.db)
        .await?;

        let mut tx = self.db.begin().await?;
        // string_id cache: value -> id. Avoids redundant upserts within this flush.
        let mut string_cache: HashMap<String, i64> = HashMap::new();
        let mut refs_inserted = 0usize;

        for file in files {
            let file_id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO files (repo_id, path, content_hash, stem, ext)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET scanned_at = NULL
                 RETURNING id",
            )
            .bind(repo_id)
            .bind(&file.rel_path)
            .bind(&file.content_hash)
            .bind(&file.stem)
            .bind(&file.ext)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO branch_files (repo_id, branch, file_id)
                 VALUES (?, ?, ?)
                 ON CONFLICT(repo_id, branch, file_id) DO NOTHING",
            )
            .bind(repo_id)
            .bind(branch)
            .bind(file_id)
            .execute(&mut *tx)
            .await?;

            for raw_ref in file.refs {
                let string_id = upsert_string_cached(
                    &mut *tx,
                    &mut string_cache,
                    &raw_ref.value,
                    self.normalize_config.as_ref(),
                )
                .await?;

                let parent_key_string_id = match &raw_ref.parent_key {
                    Some(pk) => Some(
                        upsert_string_cached(
                            &mut *tx,
                            &mut string_cache,
                            pk,
                            self.normalize_config.as_ref(),
                        )
                        .await?,
                    ),
                    None => None,
                };

                let inserted = sqlx::query_scalar::<_, i64>(
                    "INSERT INTO refs
                        (string_id, file_id, span_start, span_end, is_path, ref_kind,
                         parent_key_string_id, node_path)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(file_id, string_id, span_start) DO NOTHING
                     RETURNING id",
                )
                .bind(string_id)
                .bind(file_id)
                .bind(raw_ref.span_start as i64)
                .bind(raw_ref.span_end as i64)
                .bind(raw_ref.is_path)
                .bind(raw_ref.kind.as_u8() as i64)
                .bind(parent_key_string_id)
                .bind(&raw_ref.node_path)
                .fetch_optional(&mut *tx)
                .await?;

                if inserted.is_some() {
                    refs_inserted += 1;
                }
            }
        }

        tx.commit().await?;
        Ok(refs_inserted)
    }
}

async fn upsert_string_cached(
    tx: &mut sqlx::SqliteConnection,
    cache: &mut HashMap<String, i64>,
    value: &str,
    norm_config: Option<&NormalizeConfig>,
) -> Result<i64> {
    if let Some(&id) = cache.get(value) {
        return Ok(id);
    }

    let norm = normalize(value);
    let norm2 = norm_config.and_then(|c| normalize2(value, c));

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO strings (value, norm, norm2) VALUES (?, ?, ?)
         ON CONFLICT(value) DO UPDATE SET norm = excluded.norm, norm2 = excluded.norm2
         RETURNING id",
    )
    .bind(value)
    .bind(&norm)
    .bind(norm2.as_deref())
    .fetch_one(tx)
    .await?;

    cache.insert(value.to_string(), id);
    Ok(id)
}

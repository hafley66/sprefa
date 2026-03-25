use std::path::Path;

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_config::{CompiledFilter, RepoConfig};
use sprefa_extract::Extractor;

/// Embedded at compile time from the HEAD commit of this repo.
/// If built outside git, falls back to "unknown".
const BINARY_HASH: &str = env!("SPREFA_GIT_HASH");

pub struct Scanner {
    pub extractors: Vec<Box<dyn Extractor>>,
    pub db: SqlitePool,
    pub normalize_config: Option<sprefa_config::NormalizeConfig>,
    pub global_filter: Option<sprefa_config::FilterConfig>,
}

pub struct ScanResult {
    pub repo: String,
    pub branch: String,
    pub files_scanned: usize,
    pub refs_inserted: usize,
    pub files_skipped: usize,
}

impl Scanner {
    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let scan_ctx = sprefa_cache::load_scan_context(&self.db, &config.name, BINARY_HASH).await?;

        let (files_scanned, extracted) = sprefa_index::extract(
            Path::new(&config.path),
            compiled_filter.as_ref(),
            &self.extractors,
            &scan_ctx.skip_set,
        )?;

        let files_skipped = extracted.iter().filter(|f| f.was_skipped).count();

        tracing::info!(
            "{}/{}: {} files ({} skipped, binary={})",
            config.name, branch, files_scanned, files_skipped,
            &BINARY_HASH[..8.min(BINARY_HASH.len())],
        );

        let refs_inserted = sprefa_cache::flush(
            &self.db,
            config,
            branch,
            extracted,
            self.normalize_config.as_ref(),
            BINARY_HASH,
        ).await?;

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned,
            refs_inserted,
            files_skipped,
        })
    }
}

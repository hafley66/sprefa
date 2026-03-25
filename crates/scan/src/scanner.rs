use std::path::Path;

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_config::{CompiledFilter, RepoConfig};
use sprefa_extract::Extractor;

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
}

impl Scanner {
    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let (files_scanned, extracted) = sprefa_index::extract(
            Path::new(&config.path),
            compiled_filter.as_ref(),
            &self.extractors,
        )?;

        tracing::info!("{}/{}: {} files", config.name, branch, files_scanned);

        let refs_inserted = sprefa_cache::flush(
            &self.db,
            config,
            branch,
            extracted,
            self.normalize_config.as_ref(),
        ).await?;

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned,
            refs_inserted,
        })
    }
}

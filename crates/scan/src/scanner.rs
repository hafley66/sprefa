use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_config::{CompiledFilter, RepoConfig};
use sprefa_extract::{ExtractContext, Extractor};

/// Embedded at compile time from the HEAD commit of this repo.
/// If built outside git, falls back to "unknown".
const BINARY_HASH: &str = env!("SPREFA_GIT_HASH");

pub struct Scanner {
    pub extractors: Arc<Vec<Box<dyn Extractor>>>,
    pub db: SqlitePool,
    pub normalize_config: Option<sprefa_config::NormalizeConfig>,
    pub global_filter: Option<sprefa_config::FilterConfig>,
    pub link_rules: Vec<sprefa_cache::LinkRule>,
}

pub struct ScanResult {
    pub repo: String,
    pub branch: String,
    pub files_scanned: usize,
    pub refs_inserted: usize,
    pub files_skipped: usize,
    pub targets_resolved: usize,
    pub links_created: usize,
}

impl Scanner {
    #[tracing::instrument(skip(self, config), fields(repo = %config.name, branch = %branch))]
    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let scan_ctx = sprefa_cache::load_scan_context(&self.db, &config.name, BINARY_HASH).await?;

        let repo_path = PathBuf::from(&config.path);
        let extractors = Arc::clone(&self.extractors);
        let skip_set = scan_ctx.skip_set;
        let repo_name = config.name.clone();
        let branch_name = branch.to_string();

        let (files_scanned, extracted) = tokio::task::spawn_blocking(move || {
            let ctx = ExtractContext {
                repo: Some(&repo_name),
                branch: Some(&branch_name),
                tags: &[],
            };
            sprefa_index::extract(&repo_path, compiled_filter.as_ref(), &extractors, &skip_set, &ctx)
        })
        .await??;

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

        let targets_resolved = sprefa_cache::resolve_import_targets(&self.db, &config.name).await?;
        let links_created = sprefa_cache::resolve_match_links(&self.db, &config.name, &self.link_rules).await?;

        tracing::info!(
            "{}/{}: {} refs, {} import targets resolved, {} match links",
            config.name, branch, refs_inserted, targets_resolved, links_created,
        );

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned,
            refs_inserted,
            files_skipped,
            targets_resolved,
            links_created,
        })
    }
}

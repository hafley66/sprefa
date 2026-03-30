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
    pub files_deleted: usize,
    pub files_renamed: usize,
    pub targets_resolved: usize,
    pub links_created: usize,
    /// HEAD sha after scan, for callers to persist via upsert_repo_branch.
    pub new_git_hash: Option<String>,
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

        // Read HEAD sha for callers to persist (only for committed branches).
        let new_git_hash = if !sprefa_cache::is_wt_branch(branch) {
            let repo_path = PathBuf::from(&config.path);
            tokio::task::spawn_blocking(move || -> Option<String> {
                let repo = git2::Repository::open(&repo_path).ok()?;
                let head = repo.head().ok()?.peel_to_commit().ok()?;
                Some(head.id().to_string())
            }).await.ok().flatten()
        } else {
            None
        };

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned,
            refs_inserted,
            files_skipped,
            files_deleted: 0,
            files_renamed: 0,
            targets_resolved,
            links_created,
            new_git_hash,
        })
    }

    /// Incremental scan: only extract files changed between `old_sha` and HEAD.
    /// Falls back with an error if old_sha can't be resolved or binary hash changed
    /// (caller should retry with scan_repo).
    #[tracing::instrument(skip(self, config), fields(repo = %config.name, branch = %branch, old_sha = %old_sha))]
    pub async fn scan_diff(&self, config: &RepoConfig, branch: &str, old_sha: &str) -> Result<ScanResult> {
        // If the binary was rebuilt since last scan, some files have stale extraction
        // output. Fall back to full scan so every file gets re-extracted.
        if sprefa_cache::has_stale_scanner_hash(&self.db, &config.name, BINARY_HASH).await? {
            anyhow::bail!(
                "binary hash changed ({}), full rescan required",
                &BINARY_HASH[..8.min(BINARY_HASH.len())],
            );
        }

        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let scan_ctx = sprefa_cache::load_scan_context(&self.db, &config.name, BINARY_HASH).await?;

        let repo_path = PathBuf::from(&config.path);
        let old_sha_owned = old_sha.to_string();

        let diff = tokio::task::spawn_blocking({
            let repo_path = repo_path.clone();
            move || sprefa_index::diff_files(&repo_path, &old_sha_owned, compiled_filter.as_ref())
        }).await??;

        let files_deleted = if !diff.deleted.is_empty() {
            tracing::info!(
                "{}/{}: {} files deleted in diff",
                config.name, branch, diff.deleted.len(),
            );
            sprefa_cache::delete_branch_files_by_paths(&self.db, &config.name, branch, &diff.deleted).await?
        } else {
            0
        };

        // Pure renames (same content): update path on existing file row,
        // preserving file_id, refs, and matches. No re-extraction needed.
        let files_renamed = if !diff.renamed.is_empty() {
            tracing::info!(
                "{}/{}: {} pure renames in diff",
                config.name, branch, diff.renamed.len(),
            );
            sprefa_cache::rename_file_paths(&self.db, &config.name, &diff.renamed).await?
        } else {
            0
        };

        if diff.changed.is_empty() {
            tracing::info!("{}/{}: no modified files to extract", config.name, branch);
            return Ok(ScanResult {
                repo: config.name.clone(),
                branch: branch.to_string(),
                files_scanned: 0,
                refs_inserted: 0,
                files_skipped: 0,
                files_deleted,
                files_renamed,
                targets_resolved: 0,
                links_created: 0,
                new_git_hash: Some(diff.new_sha),
            });
        }

        let extractors = Arc::clone(&self.extractors);
        let skip_set = scan_ctx.skip_set;
        let repo_name = config.name.clone();
        let branch_name = branch.to_string();

        let (files_scanned, extracted) = tokio::task::spawn_blocking({
            let repo_path = repo_path.clone();
            move || {
                let ctx = ExtractContext {
                    repo: Some(&repo_name),
                    branch: Some(&branch_name),
                    tags: &[],
                };
                sprefa_index::extract_files(repo_path.as_path(), diff.changed, &extractors, &skip_set, &ctx)
            }
        }).await??;

        let files_skipped = extracted.iter().filter(|f| f.was_skipped).count();

        tracing::info!(
            "{}/{}: diff scan {} changed files ({} skipped, binary={})",
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
            "{}/{}: diff scan {} refs, {} targets, {} links",
            config.name, branch, refs_inserted, targets_resolved, links_created,
        );

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned,
            refs_inserted,
            files_skipped,
            files_deleted,
            files_renamed,
            targets_resolved,
            links_created,
            new_git_hash: Some(diff.new_sha),
        })
    }
}

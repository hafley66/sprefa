use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use sprefa_cache::{is_wt_rev, Store, to_file_results};
use sprefa_config::{CompiledFilter, RepoConfig};
use sprefa_extract::{ExtractContext, Extractor};

/// Embedded at compile time from the HEAD commit of this repo.
/// If built outside git, falls back to "unknown".
const BINARY_HASH: &str = env!("SPREFA_GIT_HASH");

pub struct Scanner<S: Store> {
    pub extractors: Arc<Vec<Box<dyn Extractor>>>,
    pub store: S,
    pub normalize_config: Option<sprefa_config::NormalizeConfig>,
    pub global_filter: Option<sprefa_config::FilterConfig>,
    /// Scan pairs grouped by dependency level for DAG-ordered discovery.
    /// Level 0 = rules with no cross-ref deps, level 1 = depends on level 0, etc.
    pub scan_pair_levels: Vec<Vec<sprefa_schema::rule_tables::ScanPair>>,
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

impl<S: Store> Scanner<S> {
    #[tracing::instrument(skip(self, config), fields(repo = %config.name, branch = %branch))]
    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let scan_ctx = self.store.load_scan_context(&config.name, BINARY_HASH).await?;

        let repo_path = PathBuf::from(&config.path);

        // Read all git revs (branches + tags) for ExtractContext and DB persistence.
        let git_revs = {
            let rp = repo_path.clone();
            tokio::task::spawn_blocking(move || {
                sprefa_index::read_git_revs(&rp).unwrap_or_default()
            }).await?
        };
        let tag_names: Vec<String> = git_revs.iter()
            .filter(|r| r.is_tag)
            .map(|r| r.name.clone())
            .collect();

        let extractors = Arc::clone(&self.extractors);
        let skip_set = scan_ctx.skip_set;
        let repo_name = config.name.clone();
        let branch_name = branch.to_string();

        let (files_scanned, extracted) = tokio::task::spawn_blocking(move || {
            let tag_refs: Vec<&str> = tag_names.iter().map(|s| s.as_str()).collect();
            let ctx = ExtractContext {
                repo: Some(&repo_name),
                branch: Some(&branch_name),
                tags: &tag_refs,
            };
            sprefa_index::extract(&repo_path, compiled_filter.as_ref(), &extractors, &skip_set, &ctx)
        })
        .await??;

        let files_skipped = extracted.iter().filter(|f| f.was_skipped).count();

        tracing::info!(
            "{}/{}: {} files ({} skipped, {} revs, binary={})",
            config.name, branch, files_scanned, files_skipped, git_revs.len(),
            &BINARY_HASH[..8.min(BINARY_HASH.len())],
        );

        // Convert extraction output to Store format and flush
        let file_results = to_file_results(&extracted);
        let refs_inserted = self.store.flush_batch(
            &config.name,
            branch,
            &file_results,
        ).await?;

        // Intern repo-level metadata (repo name, git revs) as linkable entities.
        // TODO: Migrate to Store trait method
        let (targets_resolved, links_created) = if let Some(pool) = self.store.sqlite_pool() {
            sprefa_cache::flush_repo_meta(
                pool,
                &config.name,
                None, // org -- not in RepoConfig yet
                &git_revs,
            ).await?;

            let targets = sprefa_cache::resolve_import_targets(pool, &config.name).await?;
            (targets, 0)
        } else {
            (0, 0)
        };

        tracing::info!(
            "{}/{}: {} refs, {} import targets resolved, {} match links",
            config.name, branch, refs_inserted, targets_resolved, links_created,
        );

        // Read HEAD sha for callers to persist (only for committed revs).
        let new_git_hash = if !is_wt_rev(branch) {
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
        if self.store.has_stale_scanner_hash(&config.name, BINARY_HASH).await? {
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

        let scan_ctx = self.store.load_scan_context(&config.name, BINARY_HASH).await?;

        let repo_path = PathBuf::from(&config.path);
        let old_sha_owned = old_sha.to_string();

        let (diff, git_revs) = tokio::task::spawn_blocking({
            let repo_path = repo_path.clone();
            move || -> Result<_> {
                let diff = sprefa_index::diff_files(&repo_path, &old_sha_owned, compiled_filter.as_ref())?;
                let revs = sprefa_index::read_git_revs(&repo_path).unwrap_or_default();
                Ok((diff, revs))
            }
        }).await??;
        let tag_names: Vec<String> = git_revs.iter()
            .filter(|r| r.is_tag)
            .map(|r| r.name.clone())
            .collect();

        let files_deleted = if !diff.deleted.is_empty() {
            tracing::info!(
                "{}/{}: {} files deleted in diff",
                config.name, branch, diff.deleted.len(),
            );
            self.store.delete_files(&config.name, branch, &diff.deleted).await?
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
            self.store.rename_files(&config.name, &diff.renamed).await?
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
                let tag_refs: Vec<&str> = tag_names.iter().map(|s| s.as_str()).collect();
                let ctx = ExtractContext {
                    repo: Some(&repo_name),
                    branch: Some(&branch_name),
                    tags: &tag_refs,
                };
                sprefa_index::extract_files(repo_path.as_path(), diff.changed, &extractors, &skip_set, &ctx)
            }
        }).await??;

        let files_skipped = extracted.iter().filter(|f| f.was_skipped).count();

        tracing::info!(
            "{}/{}: diff scan {} changed files ({} skipped, {} revs, binary={})",
            config.name, branch, files_scanned, files_skipped, git_revs.len(),
            &BINARY_HASH[..8.min(BINARY_HASH.len())],
        );

        // Convert extraction output to Store format and flush
        let file_results = to_file_results(&extracted);
        let refs_inserted = self.store.flush_batch(
            &config.name,
            branch,
            &file_results,
        ).await?;

        // Intern repo-level metadata as linkable entities.
        // TODO: Migrate to Store trait method
        let (targets_resolved, links_created) = if let Some(pool) = self.store.sqlite_pool() {
            sprefa_cache::flush_repo_meta(
                pool,
                &config.name,
                None,
                &git_revs,
            ).await?;

            let targets = sprefa_cache::resolve_import_targets(pool, &config.name).await?;
            (targets, 0)
        } else {
            (0, 0)
        };

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

    /// Scan a specific git revision (tag, branch sha) using the git blob reader.
    /// No checkout needed. Stores results with `rev` as the rev key in rev_files.
    /// Skips flush_repo_meta and resolve_import_targets (historical snapshots).
    #[tracing::instrument(skip(self, config), fields(repo = %config.name, rev = %rev))]
    pub async fn scan_rev(&self, config: &RepoConfig, rev: &str) -> Result<ScanResult> {
        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, rev);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let scan_ctx = self.store.load_scan_context(&config.name, BINARY_HASH).await?;

        let repo_path = PathBuf::from(&config.path);
        let extractors = Arc::clone(&self.extractors);
        let skip_set = scan_ctx.skip_set;
        let repo_name = config.name.clone();
        let rev_owned = rev.to_string();

        let (files_scanned, extracted) = tokio::task::spawn_blocking(move || {
            let ctx = ExtractContext {
                repo: Some(&repo_name),
                branch: Some(&rev_owned),
                tags: &[],
            };
            sprefa_index::extract_rev(
                &repo_path,
                &rev_owned,
                compiled_filter.as_ref(),
                &extractors,
                &skip_set,
                &ctx,
            )
        })
        .await??;

        let files_skipped = extracted.iter().filter(|f| f.was_skipped).count();

        tracing::info!(
            "{} @ {}: {} blobs ({} skipped, binary={})",
            config.name, rev, files_scanned, files_skipped,
            &BINARY_HASH[..8.min(BINARY_HASH.len())],
        );

        // Convert extraction output to Store format and flush
        let file_results = to_file_results(&extracted);
        let refs_inserted = self.store.flush_batch(
            &config.name,
            rev,
            &file_results,
        ).await?;

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: rev.to_string(),
            files_scanned,
            refs_inserted,
            files_skipped,
            files_deleted: 0,
            files_renamed: 0,
            targets_resolved: 0,
            links_created: 0,
            new_git_hash: None,
        })
    }

    /// Re-resolve match links for a repo without re-scanning files.
    /// Used as a second pass after all repos are scanned to pick up
    /// cross-repo links that couldn't resolve due to scan order.
    /// DEPRECATED: Link rules have been removed.
    pub async fn resolve_links(&self, _repo_name: &str) -> Result<usize> {
        Ok(0)
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use sprefa_extract::Extractor;
use sprefa_schema::RefKind;

use crate::change::{Change, DeclChange, FsChange};
use crate::diff::diff_refs;

/// Configuration for the filesystem watcher.
#[derive(Debug)]
pub struct WatchConfig {
    /// Root path of the repo being watched.
    pub root_path: PathBuf,
    /// Repo ID in the database.
    pub repo_id: i64,
    /// Debounce window for correlating events.
    pub debounce: Duration,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::new(),
            repo_id: 0,
            debounce: Duration::from_millis(100),
        }
    }
}

/// Raw event accumulated during debounce window.
#[derive(Debug)]
enum RawEvent {
    Created(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
}

/// Start watching a repo directory. Returns a stream of classified change batches.
///
/// Each batch is a debounced, correlated set of changes ready for the planner.
/// The caller receives batches on the returned channel and feeds them into
/// `plan_rewrites` + `rewrite::apply`.
#[tracing::instrument(skip(pool, extractors), fields(repo_id = config.repo_id, root = %config.root_path.display()))]
pub async fn watch(
    config: WatchConfig,
    pool: SqlitePool,
    extractors: Arc<Vec<Box<dyn Extractor>>>,
) -> Result<mpsc::Receiver<Vec<Change>>> {
    let (change_tx, change_rx) = mpsc::channel::<Vec<Change>>(32);
    let (event_tx, mut event_rx) = mpsc::channel::<RawEvent>(512);

    // Spawn the notify watcher on a blocking thread.
    let root = config.root_path.clone();
    let _watcher = spawn_notify_watcher(root.clone(), event_tx)?;

    // Spawn the debounce + classify loop.
    let debounce = config.debounce;
    tokio::spawn(async move {
        // Keep watcher alive for the lifetime of this task.
        let _watcher = _watcher;
        loop {
            let batch = collect_batch(&mut event_rx, debounce).await;
            if batch.is_empty() {
                continue;
            }

            match classify_batch(
                &batch,
                &config.root_path,
                config.repo_id,
                &pool,
                &extractors,
            )
            .await
            {
                Ok(changes) if !changes.is_empty() => {
                    if change_tx.send(changes).await.is_err() {
                        break; // receiver dropped
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("classify batch: {}", e);
                }
            }
        }
    });

    Ok(change_rx)
}

fn spawn_notify_watcher(
    root: PathBuf,
    tx: mpsc::Sender<RawEvent>,
) -> Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };

        for path in event.paths {
            let raw = match event.kind {
                EventKind::Create(_) => RawEvent::Created(path),
                EventKind::Remove(_) => RawEvent::Removed(path),
                EventKind::Modify(_) => RawEvent::Modified(path),
                _ => continue,
            };
            // Best-effort send. If the channel is full, we drop the event
            // rather than blocking the OS event thread.
            let _ = tx.try_send(raw);
        }
    })?;

    watcher.watch(&root, RecursiveMode::Recursive)?;
    Ok(watcher)
}

/// Accumulate raw events until the debounce window passes with no new events.
async fn collect_batch(
    rx: &mut mpsc::Receiver<RawEvent>,
    debounce: Duration,
) -> Vec<RawEvent> {
    // Wait for the first event (blocks until something happens).
    let first = match rx.recv().await {
        Some(e) => e,
        None => return vec![], // channel closed
    };

    let mut batch = vec![first];

    // Keep draining until debounce window passes with no new events.
    loop {
        match tokio::time::timeout(debounce, rx.recv()).await {
            Ok(Some(event)) => batch.push(event),
            _ => break, // timeout or channel closed
        }
    }

    batch
}

/// Classify a debounced batch of raw events into semantic FsChanges,
/// then derive DeclChanges for content changes.
#[tracing::instrument(skip(batch, root, pool, extractors), fields(event_count = batch.len()))]
async fn classify_batch(
    batch: &[RawEvent],
    root: &Path,
    repo_id: i64,
    pool: &SqlitePool,
    extractors: &[Box<dyn Extractor>],
) -> Result<Vec<Change>> {
    let mut created: HashMap<PathBuf, String> = HashMap::new(); // path -> content_hash
    let mut removed: Vec<PathBuf> = Vec::new();
    let mut modified: Vec<PathBuf> = Vec::new();

    for event in batch {
        match event {
            RawEvent::Created(p) => {
                if p.is_file() {
                    if let Some(hash) = hash_file(p) {
                        created.insert(p.clone(), hash);
                    }
                }
            }
            RawEvent::Removed(p) => {
                removed.push(p.clone());
            }
            RawEvent::Modified(p) => {
                if p.is_file() {
                    modified.push(p.clone());
                }
            }
        }
    }

    let mut changes: Vec<Change> = Vec::new();

    // Query DB for removed files' content_hash to correlate moves.
    let mut removed_info: HashMap<String, (i64, PathBuf)> = HashMap::new(); // hash -> (file_id, path)
    for path in &removed {
        let rel = rel_path(root, path);
        if let Some((file_id, hash)) = lookup_file(pool, repo_id, &rel).await? {
            removed_info.insert(hash, (file_id, path.clone()));
        }
    }

    // Match created files against removed files by content_hash.
    let mut matched_creates: Vec<PathBuf> = Vec::new();
    for (create_path, create_hash) in &created {
        if let Some((file_id, old_path)) = removed_info.remove(create_hash) {
            changes.push(
                FsChange::Move {
                    file_id,
                    old_path: old_path.to_string_lossy().to_string(),
                    new_path: create_path.to_string_lossy().to_string(),
                }
                .into(),
            );
            matched_creates.push(create_path.clone());
        }
    }

    // Unmatched removes = deletes.
    for (_hash, (file_id, path)) in &removed_info {
        changes.push(
            FsChange::Delete {
                file_id: *file_id,
                path: path.to_string_lossy().to_string(),
            }
            .into(),
        );
    }

    // Unmatched creates = new files.
    for (path, _hash) in &created {
        if !matched_creates.contains(path) {
            changes.push(
                FsChange::Create {
                    path: path.to_string_lossy().to_string(),
                }
                .into(),
            );
        }
    }

    // Modified files = content changes. Re-extract and diff.
    for path in &modified {
        let rel = rel_path(root, path);
        if let Some((file_id, _old_hash)) = lookup_file(pool, repo_id, &rel).await? {
            changes.push(
                FsChange::ContentChange {
                    file_id,
                    path: path.to_string_lossy().to_string(),
                }
                .into(),
            );

            // Re-extract the file and diff against old refs.
            let decl_changes =
                extract_and_diff(pool, file_id, path, extractors).await?;
            changes.extend(decl_changes.into_iter().map(Change::from));
        }
    }

    Ok(changes)
}

/// Hash a file using xxh3_128, matching the index crate's approach.
fn hash_file(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mmap = unsafe { memmap2::Mmap::map(&file).ok()? };
    Some(format!("{:x}", xxhash_rust::xxh3::xxh3_128(&mmap)))
}

/// Convert absolute path to repo-relative path.
fn rel_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Look up a file in the DB by repo_id + relative path.
/// Returns (file_id, content_hash) if found.
async fn lookup_file(
    pool: &SqlitePool,
    repo_id: i64,
    rel_path: &str,
) -> Result<Option<(i64, String)>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, content_hash FROM files WHERE repo_id = ? AND path = ?",
    )
    .bind(repo_id)
    .bind(rel_path)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Re-extract a modified file and diff its declarations against the DB.
async fn extract_and_diff(
    pool: &SqlitePool,
    file_id: i64,
    abs_path: &Path,
    extractors: &[Box<dyn Extractor>],
) -> Result<Vec<DeclChange>> {
    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let extractor = extractors.iter().find(|e| e.extensions().contains(&ext));
    let Some(extractor) = extractor else {
        return Ok(vec![]);
    };

    let content = match std::fs::read(abs_path) {
        Ok(c) => c,
        Err(_) => return Ok(vec![]), // file may have been deleted between events
    };

    let new_refs = extractor.extract(&content, &abs_path.to_string_lossy());

    // Load old refs from DB for this file (declaration kinds only).
    let old_refs = load_decl_refs(pool, file_id).await?;

    Ok(diff_refs(file_id, &old_refs, &new_refs))
}

/// Load existing refs for a file from the DB, filtered to declaration kinds.
async fn load_decl_refs(
    pool: &SqlitePool,
    file_id: i64,
) -> Result<Vec<sprefa_extract::RawRef>> {
    use sprefa_extract::RawRef;

    let decl_kinds: Vec<i64> = DECL_KINDS
        .iter()
        .map(|k| k.as_u8() as i64)
        .collect();

    // Build a query that fetches all decl refs for this file.
    // We need: ref_kind, value, span_start, span_end
    let rows: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT r.ref_kind, s.value, r.span_start, r.span_end
         FROM refs r
         JOIN strings s ON r.string_id = s.id
         WHERE r.file_id = ?
         ORDER BY r.span_start",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter(|(kind, _, _, _)| decl_kinds.contains(kind))
        .filter_map(|(kind, value, start, end)| {
            Some(RawRef {
                kind: RefKind::from_u8(kind as u8)?,
                value,
                span_start: start as u32,
                span_end: end as u32,
                is_path: false,
                parent_key: None,
                node_path: None,
            })
        })
        .collect())
}

const DECL_KINDS: &[RefKind] = &[
    RefKind::ExportName,
    RefKind::RsDeclare,
    RefKind::RsMod,
];

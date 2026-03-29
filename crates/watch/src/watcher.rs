use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use notify::{
    event::{ModifyKind, RenameMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use sprefa_extract::{kind, ExtractContext, Extractor, RawRef};

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
    /// Working-tree branch name (e.g. "main+wt"). The watcher updates
    /// branch_files for this branch on file create/delete events.
    pub wt_branch: Option<String>,
    /// When true, the watcher drains events without classifying them.
    /// Used to suppress rewrite activity during external checkout updates.
    pub pause: Arc<AtomicBool>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::new(),
            repo_id: 0,
            debounce: Duration::from_millis(100),
            wt_branch: None,
            pause: Arc::new(AtomicBool::new(false)),
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

            if config.pause.load(Ordering::Relaxed) {
                tracing::debug!(
                    repo_id = config.repo_id,
                    dropped = batch.len(),
                    "watcher paused, draining events"
                );
                continue;
            }

            match classify_batch(
                &batch,
                &config.root_path,
                config.repo_id,
                &pool,
                &extractors,
                config.wt_branch.as_deref(),
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

        match event.kind {
            // Rename events (macOS FSEvents fires these instead of Create+Remove for mv).
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                // Both paths in one event: [old_path, new_path].
                if event.paths.len() >= 2 {
                    let _ = tx.try_send(RawEvent::Removed(event.paths[0].clone()));
                    let _ = tx.try_send(RawEvent::Created(event.paths[1].clone()));
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Removed(path));
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Created(path));
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Any | RenameMode::Other)) => {
                // Ambiguous rename -- treat as both removed and created so
                // content-hash matching can sort it out.
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Removed(path.clone()));
                    let _ = tx.try_send(RawEvent::Created(path));
                }
            }
            // Standard file events.
            EventKind::Create(_) => {
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Created(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Removed(path));
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    let _ = tx.try_send(RawEvent::Modified(path));
                }
            }
            _ => {}
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
#[tracing::instrument(skip(batch, root, pool, extractors, wt_branch), fields(event_count = batch.len()))]
async fn classify_batch(
    batch: &[RawEvent],
    root: &Path,
    repo_id: i64,
    pool: &SqlitePool,
    extractors: &[Box<dyn Extractor>],
    wt_branch: Option<&str>,
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

    // Deduplicate created vs modified using the DB as source of truth.
    // macOS can report both Create and Modify for the same path in one batch
    // (e.g. git checkout, atomic saves). Resolution:
    //   - File exists in DB: it's a modify, not a create. Drop from created.
    //   - File not in DB: it's a create, not a modify. Drop from modified.
    let mut drop_from_created: Vec<PathBuf> = Vec::new();
    let mut drop_from_modified: Vec<PathBuf> = Vec::new();
    for path in created.keys() {
        let rel = rel_path(root, path);
        if let Ok(Some(_)) = lookup_file(pool, repo_id, &rel).await {
            // Exists in DB -- treat as modify
            drop_from_created.push(path.clone());
            if !modified.contains(path) {
                modified.push(path.clone());
            }
            tracing::debug!(path = %path.display(), "classify: promoted create -> modify (file exists in DB)");
        } else if modified.contains(path) {
            // Doesn't exist in DB -- treat as create, remove from modified
            drop_from_modified.push(path.clone());
        }
    }
    for path in &drop_from_created {
        created.remove(path);
    }
    modified.retain(|p| !drop_from_modified.contains(p));

    let mut changes: Vec<Change> = Vec::new();

    tracing::info!(
        created_count = created.len(),
        removed_count = removed.len(),
        modified_count = modified.len(),
        "classify: raw event counts"
    );

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

            // Update DB so future lookups find this file at its new path.
            let new_rel = rel_path(root, create_path);
            sqlx::query("UPDATE files SET path = ? WHERE id = ?")
                .bind(&new_rel)
                .bind(file_id)
                .execute(pool)
                .await?;
            tracing::debug!(file_id, old = %old_path.display(), new = %new_rel, "db: file path updated");
        }
    }

    // Same-path delete+create with different content = content change.
    // This happens on macOS when editors do atomic saves (delete + create).
    let mut same_path_creates: Vec<PathBuf> = Vec::new();
    let mut same_path_removes: Vec<String> = Vec::new();
    for (create_path, _create_hash) in &created {
        if matched_creates.contains(create_path) {
            continue;
        }
        // Check if a remove exists at the same path (by comparing against DB path)
        let create_rel = rel_path(root, create_path);
        let matching_remove = removed_info.iter()
            .find(|(_hash, (_fid, rem_path))| rel_path(root, rem_path) == create_rel);
        if let Some((hash, (file_id, _rem_path))) = matching_remove {
            tracing::info!(file_id, path = %create_rel, "classify: same-path content change detected");
            changes.push(
                FsChange::ContentChange {
                    file_id: *file_id,
                    path: create_path.to_string_lossy().to_string(),
                }
                .into(),
            );
            let decl_changes =
                extract_and_diff(pool, *file_id, create_path, extractors).await?;
            changes.extend(decl_changes.into_iter().map(Change::from));
            if let Some(new_hash) = hash_file(create_path) {
                update_content_hash(pool, *file_id, &new_hash).await?;
            }
            same_path_creates.push(create_path.clone());
            same_path_removes.push(hash.clone());
        }
    }
    for hash in &same_path_removes {
        removed_info.remove(hash);
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

        // Remove from working-tree branch_files.
        if let Some(wt) = wt_branch {
            sqlx::query(
                "DELETE FROM branch_files WHERE repo_id = ? AND branch = ? AND file_id = ?",
            )
            .bind(repo_id)
            .bind(wt)
            .bind(*file_id)
            .execute(pool)
            .await?;

            tracing::debug!(file_id, branch = wt, "wt: unlinked deleted file");
        }
    }

    // Unmatched creates = new files.
    for (path, hash) in &created {
        if !matched_creates.contains(path) && !same_path_creates.contains(path) {
            let rel = rel_path(root, path);
            changes.push(
                FsChange::Create {
                    path: path.to_string_lossy().to_string(),
                }
                .into(),
            );

            // Insert into files + branch_files for the working-tree branch.
            if let Some(wt) = wt_branch {
                let stem = Path::new(&rel).file_stem().map(|s| s.to_string_lossy().to_string());
                let ext = Path::new(&rel).extension().map(|s| s.to_string_lossy().to_string());
                let file_id: i64 = sqlx::query_scalar(
                    "INSERT INTO files (repo_id, path, content_hash, stem, ext)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET stem = excluded.stem
                     RETURNING id",
                )
                .bind(repo_id)
                .bind(&rel)
                .bind(hash)
                .bind(stem.as_deref())
                .bind(ext.as_deref())
                .fetch_one(pool)
                .await?;

                sqlx::query(
                    "INSERT OR IGNORE INTO branch_files (repo_id, branch, file_id) VALUES (?, ?, ?)",
                )
                .bind(repo_id)
                .bind(wt)
                .bind(file_id)
                .execute(pool)
                .await?;

                tracing::debug!(file_id, branch = wt, path = %rel, "wt: linked new file");
            }
        }
    }

    // Modified files = content changes. Re-extract, diff decls, and sync DB.
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

            // Re-extract the file, diff decls, and replace refs in DB.
            let decl_changes =
                extract_and_diff(pool, file_id, path, extractors).await?;
            changes.extend(decl_changes.into_iter().map(Change::from));

            // Update content_hash so future move correlation uses the new hash.
            if let Some(new_hash) = hash_file(path) {
                update_content_hash(pool, file_id, &new_hash).await?;
            }
        }
    }

    Ok(changes)
}

/// Update content_hash for a file, handling the UNIQUE(repo_id, path, content_hash)
/// constraint by first deleting any stale duplicate rows at the same path.
async fn update_content_hash(pool: &SqlitePool, file_id: i64, new_hash: &str) -> Result<()> {
    // Delete refs for stale duplicate rows first (FK constraint), then the rows themselves.
    sqlx::query(
        "DELETE FROM refs WHERE file_id IN (SELECT id FROM files WHERE id != ? AND repo_id = (SELECT repo_id FROM files WHERE id = ?) AND path = (SELECT path FROM files WHERE id = ?))"
    )
    .bind(file_id)
    .bind(file_id)
    .bind(file_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "DELETE FROM files WHERE id != ? AND repo_id = (SELECT repo_id FROM files WHERE id = ?) AND path = (SELECT path FROM files WHERE id = ?)"
    )
    .bind(file_id)
    .bind(file_id)
    .bind(file_id)
    .execute(pool)
    .await?;

    sqlx::query("UPDATE files SET content_hash = ? WHERE id = ?")
        .bind(new_hash)
        .bind(file_id)
        .execute(pool)
        .await?;
    Ok(())
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

/// Re-extract a modified file, diff its declarations against the DB,
/// and replace all refs in the DB with the freshly extracted set.
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

    let ctx = ExtractContext::default();
    let new_refs = extractor.extract(&content, &abs_path.to_string_lossy(), &ctx);

    // Load old decl refs from DB for diffing (declaration kinds only).
    let old_refs = load_decl_refs(pool, file_id).await?;
    let decl_changes = diff_refs(file_id, &old_refs, &new_refs);

    // Replace ALL refs in DB for this file so spans and values stay current.
    replace_file_refs(pool, file_id, &new_refs).await?;

    Ok(decl_changes)
}

/// Load existing refs for a file from the DB, filtered to declaration kinds.
async fn load_decl_refs(
    pool: &SqlitePool,
    file_id: i64,
) -> Result<Vec<RawRef>> {
    let placeholders = DECL_KINDS.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT m.kind, s.value, r.span_start, r.span_end, m.rule_name
         FROM refs r
         JOIN strings s ON r.string_id = s.id
         JOIN matches m ON m.ref_id = r.id
         WHERE r.file_id = ? AND m.kind IN ({})
         ORDER BY r.span_start",
        placeholders
    );
    let mut query = sqlx::query_as::<_, (String, String, i64, i64, String)>(&sql).bind(file_id);
    for kind in DECL_KINDS {
        query = query.bind(*kind);
    }
    let rows = query.fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(kind, value, start, end, rule_name)| {
            RawRef {
                kind,
                rule_name,
                value,
                span_start: start as u32,
                span_end: end as u32,
                is_path: false,
                parent_key: None,
                node_path: None,
            }
        })
        .collect())
}

/// Replace all refs for a file in the DB with a freshly extracted set.
///
/// Runs in a single transaction: delete old refs, bulk-insert strings,
/// bulk-insert new refs. Keeps the DB in sync with the filesystem after
/// content changes or rewrites.
async fn replace_file_refs(
    pool: &SqlitePool,
    file_id: i64,
    new_refs: &[RawRef],
) -> Result<()> {
    let mut tx = pool.begin().await?;

    if new_refs.is_empty() {
        // No new refs -- prune all existing refs + matches for this file.
        sqlx::query("DELETE FROM matches WHERE ref_id IN (SELECT id FROM refs WHERE file_id = ?)")
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM refs WHERE file_id = ?")
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(());
    }

    // Collect unique string values (ref values + parent keys).
    let mut seen = std::collections::HashSet::new();
    let mut unique_strings: Vec<&str> = Vec::new();
    for r in new_refs {
        if seen.insert(r.value.as_str()) {
            unique_strings.push(&r.value);
        }
        if let Some(pk) = &r.parent_key {
            if seen.insert(pk.as_str()) {
                unique_strings.push(pk);
            }
        }
    }

    // Bulk-insert strings. norm = lowercase(value) matches the index crate.
    for chunk in unique_strings.chunks(500) {
        let ph = chunk.iter().map(|_| "(?,?)").collect::<Vec<_>>().join(",");
        let sql = format!("INSERT OR IGNORE INTO strings (value, norm) VALUES {ph}");
        let mut q = sqlx::query(&sql);
        for v in chunk {
            q = q.bind(*v).bind(v.trim().to_lowercase());
        }
        q.execute(&mut *tx).await?;
    }

    // Bulk look up string IDs.
    let mut string_ids: HashMap<String, i64> = HashMap::with_capacity(unique_strings.len());
    for chunk in unique_strings.chunks(500) {
        let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, value FROM strings WHERE value IN ({ph})");
        let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
        for v in chunk {
            q = q.bind(*v);
        }
        for (id, value) in q.fetch_all(&mut *tx).await? {
            string_ids.insert(value, id);
        }
    }

    // Resolve all foreign keys before bulk operations.
    struct ResolvedRef<'a> {
        string_id: i64,
        span_start: i64,
        span_end: i64,
        is_path: bool,
        parent_key_string_id: Option<i64>,
        node_path: Option<String>,
        kind: &'a str,
        rule_name: &'a str,
    }
    let resolved: Vec<ResolvedRef<'_>> = new_refs
        .iter()
        .filter_map(|r| {
            let &string_id = string_ids.get(&r.value)?;
            Some(ResolvedRef {
                string_id,
                span_start: r.span_start as i64,
                span_end: r.span_end as i64,
                is_path: r.is_path,
                parent_key_string_id: r.parent_key.as_ref().and_then(|pk| string_ids.get(pk).copied()),
                node_path: r.node_path.clone(),
                kind: &r.kind,
                rule_name: &r.rule_name,
            })
        })
        .collect();

    // Upsert refs: existing refs keep their IDs, new refs get inserted.
    // UNIQUE(file_id, string_id, span_start) is the stable identity.
    for chunk in resolved.chunks(200) {
        let ph = chunk.iter().map(|_| "(?,?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO refs
             (string_id, file_id, span_start, span_end, is_path,
              parent_key_string_id, node_path)
             VALUES {ph}
             ON CONFLICT(file_id, string_id, span_start) DO UPDATE SET
               span_end = excluded.span_end,
               is_path = excluded.is_path,
               parent_key_string_id = excluded.parent_key_string_id,
               node_path = excluded.node_path"
        );
        let mut q = sqlx::query(&sql);
        for r in chunk {
            q = q
                .bind(r.string_id)
                .bind(file_id)
                .bind(r.span_start)
                .bind(r.span_end)
                .bind(r.is_path)
                .bind(r.parent_key_string_id)
                .bind(r.node_path.as_deref());
        }
        q.execute(&mut *tx).await?;
    }

    // Prune stale refs: refs in DB for this file that aren't in the new set.
    let new_keys: HashSet<(i64, i64)> = resolved.iter()
        .map(|r| (r.string_id, r.span_start))
        .collect();
    let all_refs: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT id, string_id, span_start FROM refs WHERE file_id = ?",
    )
    .bind(file_id)
    .fetch_all(&mut *tx)
    .await?;
    let stale_ids: Vec<i64> = all_refs.iter()
        .filter(|(_, sid, ss)| !new_keys.contains(&(*sid, *ss)))
        .map(|(id, _, _)| *id)
        .collect();
    // Delete matches + refs for stale refs (cascading: matches first).
    if !stale_ids.is_empty() {
        for chunk in stale_ids.chunks(500) {
            let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("DELETE FROM matches WHERE ref_id IN ({ph})");
            let mut q = sqlx::query(&sql);
            for id in chunk { q = q.bind(id); }
            q.execute(&mut *tx).await?;

            let sql = format!("DELETE FROM refs WHERE id IN ({ph})");
            let mut q = sqlx::query(&sql);
            for id in chunk { q = q.bind(id); }
            q.execute(&mut *tx).await?;
        }
    }

    // Build ref ID map from surviving + new refs.
    let ref_id_map: HashMap<(i64, i64), i64> = all_refs.iter()
        .filter(|(_, sid, ss)| new_keys.contains(&(*sid, *ss)))
        .map(|(id, sid, ss)| ((*sid, *ss), *id))
        .collect();

    // Upsert matches: UNIQUE(ref_id, rule_name, kind) is the stable identity.
    // Matches have no mutable columns beyond the key, so ON CONFLICT is a no-op
    // update -- the point is to not fail on existing rows.
    for chunk in resolved.chunks(200) {
        let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO matches (ref_id, rule_name, kind) VALUES {ph}
             ON CONFLICT(ref_id, rule_name, kind) DO NOTHING"
        );
        let mut q = sqlx::query(&sql);
        for r in chunk {
            let ref_id = ref_id_map.get(&(r.string_id, r.span_start)).copied().unwrap_or(0);
            q = q.bind(ref_id).bind(r.rule_name).bind(r.kind);
        }
        q.execute(&mut *tx).await?;
    }

    // Prune stale matches: matches on surviving refs whose (rule_name, kind)
    // combo is no longer in the new extraction.
    let surviving_ref_ids: Vec<i64> = all_refs.iter()
        .filter(|(_, sid, ss)| new_keys.contains(&(*sid, *ss)))
        .map(|(id, _, _)| *id)
        .collect();
    if !surviving_ref_ids.is_empty() {
        // Build set of (ref_id, rule_name, kind) from new extraction.
        let new_match_keys: HashSet<(i64, &str, &str)> = resolved.iter()
            .filter_map(|r| {
                let ref_id = ref_id_map.get(&(r.string_id, r.span_start))?;
                Some((*ref_id, r.rule_name, r.kind))
            })
            .collect();

        // Load existing matches for surviving refs.
        for chunk in surviving_ref_ids.chunks(500) {
            let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT id, ref_id, rule_name, kind FROM matches WHERE ref_id IN ({ph})"
            );
            let mut q = sqlx::query_as::<_, (i64, i64, String, String)>(&sql);
            for id in chunk { q = q.bind(id); }
            let existing = q.fetch_all(&mut *tx).await?;

            let stale_match_ids: Vec<i64> = existing.iter()
                .filter(|(_, rid, rn, k)| {
                    !new_match_keys.contains(&(*rid, rn.as_str(), k.as_str()))
                })
                .map(|(id, _, _, _)| *id)
                .collect();

            if !stale_match_ids.is_empty() {
                let ph = stale_match_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!("DELETE FROM matches WHERE id IN ({ph})");
                let mut q = sqlx::query(&sql);
                for id in &stale_match_ids { q = q.bind(id); }
                q.execute(&mut *tx).await?;
            }
        }
    }

    tx.commit().await?;
    tracing::debug!(file_id, ref_count = new_refs.len(), "db: refs upserted");
    Ok(())
}

const DECL_KINDS: &[&str] = &[
    kind::EXPORT_NAME,
    kind::RS_DECLARE,
    kind::RS_MOD,
    kind::IMPORT_NAME,
];

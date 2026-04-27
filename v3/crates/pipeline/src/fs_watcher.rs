//! Phase F (sprefa-3n6): notify-glue. One concrete producer for the
//! invalidation channel; not a trait.
//!
//! `spawn_fs_watcher(repo, root, sender, debounce)` walks the worktree
//! at `root`, debounces FS events into a single `Change::FileBatch`
//! per quiesce window, and forwards it on the broadcast `sender`.
//!
//! The function is small on purpose. Other producers (.git/refs poller,
//! pack-file watcher, polling-only fallback) are sibling functions that
//! push the same `Change` shape. No abstraction tax.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::broadcast;

use crate::invalidate::Change;

/// Spawn an OS-event watcher for `root`. Each quiesce window emits one
/// coalesced `Change::FileBatch` tagged with `(repo, "wt")` — the
/// worktree rev label. RevHead changes (branch hop) are NOT detected
/// here; a sibling poller covers that.
///
/// Returns a [`std::thread::JoinHandle`] paired with a guard that
/// stops the watcher when dropped. notify spawns its own thread; the
/// caller drops the guard to tear down.
///
/// macOS granularity caveat: FSEvents reports parent directories, not
/// individual files, so a file write on `src/lib.rs` may surface as a
/// `Change::FileBatch` carrying `src/` rather than `src/lib.rs`. The
/// downstream invalidator treats coarse paths as exact-match keys, so
/// dir-only events under-evict on macOS today. Tightening this requires
/// either kqueue (file granularity, lower limits) or a dir-rescan after
/// each event. Filed as Phase F.2.
pub fn spawn_fs_watcher(
    repo: Arc<str>,
    root: PathBuf,
    sender: broadcast::Sender<Change>,
    debounce: Duration,
) -> notify::Result<FsWatcherGuard> {
    // Canonicalize so events (which arrive with the OS's canonical
    // form, e.g. /private/var/... on macOS) strip cleanly against the
    // configured root.
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let root_for_event = root.clone();
    let mut debouncer = new_debouncer(debounce, move |res: notify_debouncer_mini::DebounceEventResult| {
        let Ok(events) = res else { return };
        let mut paths: Vec<PathBuf> = Vec::with_capacity(events.len());
        for ev in events {
            // notify-debouncer-mini collapses every kind into Any/AnyContinuous.
            // For invalidation purposes (was content, did it change),
            // both are signal — we evict the entry and let the next
            // pipeline subscription recompute.
            if !matches!(ev.kind, DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous) {
                continue;
            }
            // Strip the watcher root so paths align with the cursor's
            // `fs` field (which is repo-relative in normal pipelines).
            let rel = ev.path.strip_prefix(&root_for_event).ok().map(|p| p.to_path_buf());
            paths.push(rel.unwrap_or(ev.path));
        }
        if paths.is_empty() { return; }
        let _ = sender.send(Change::FileBatch {
            repo: repo.clone(),
            rev: Arc::from("wt"),
            paths,
        });
    })?;

    debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;

    Ok(FsWatcherGuard { _debouncer: debouncer })
}

/// RAII handle. Dropping it stops the watcher thread.
pub struct FsWatcherGuard {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl std::fmt::Debug for FsWatcherGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsWatcherGuard").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invalidate::ChangeSubject;
    use std::time::Duration;

    fn tempdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("sprefa_fs_watcher_{name}_{pid}_{nonce}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fires_change_on_file_write() {
        let dir = tempdir("write");
        let subject = ChangeSubject::new();
        let mut rx = subject.subscribe();

        let _guard = spawn_fs_watcher(
            Arc::from("r"),
            dir.clone(),
            subject.sender(),
            Duration::from_millis(50),
        )
        .expect("watcher spawned");

        // Notify needs a moment to register the watch.
        tokio::time::sleep(Duration::from_millis(100)).await;

        let target = dir.join("a.rs");
        std::fs::write(&target, b"fn main() {}").unwrap();

        let recv = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;
        let change = recv.expect("watcher fired in time").expect("subject open");
        match change {
            Change::FileBatch { repo, rev, paths } => {
                assert_eq!(&*repo, "r");
                assert_eq!(&*rev, "wt");
                // macOS FSEvents may report the parent dir rather than
                // the file. Accept either: file-granular (ends with
                // a.rs) or dir-granular (empty path = root itself).
                let saw_file_or_root = paths
                    .iter()
                    .any(|p| p.ends_with("a.rs") || p.as_os_str().is_empty());
                assert!(saw_file_or_root, "got paths: {:?}", paths);
            }
            other => panic!("expected FileBatch, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

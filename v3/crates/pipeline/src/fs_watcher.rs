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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::broadcast;

use crate::invalidate::Change;

/// Ignore-matcher rooted at `root`. Combines:
/// - `<root>/.gitignore` (the common case — `target/`, `node_modules/`, …),
/// - `<root>/.git/info/exclude` (git-managed local excludes),
/// - the global `core.excludesFile` (if set).
///
/// Nested `.gitignore` files inside subdirectories are NOT loaded — a
/// repo-root-only matcher catches the bulk of build-output noise without
/// the cost of walking every directory at watcher start. Extend if a
/// nested gitignore proves load-bearing.
fn build_ignore_matcher(root: &Path) -> Gitignore {
    let mut b = GitignoreBuilder::new(root);
    let _ = b.add(root.join(".gitignore"));
    let _ = b.add(root.join(".git").join("info").join("exclude"));
    if let Some(path) = global_gitignore_path() {
        let _ = b.add(path);
    }
    b.build().unwrap_or_else(|_| Gitignore::empty())
}

fn global_gitignore_path() -> Option<PathBuf> {
    use std::process::Command;
    let out = Command::new("git")
        .args(["config", "--get", "core.excludesFile"])
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let raw = String::from_utf8(out.stdout).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() { return None; }
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(rest))
    } else {
        Some(PathBuf::from(trimmed))
    };
    expanded.filter(|p| p.exists())
}

/// True if `rel` (a path relative to the watcher root) is in `.git/`
/// or matches the gitignore set. Always-skip `.git/` because notify
/// recursive watchers trip on every fetch / index update otherwise.
fn should_skip(rel: &Path, ignore: &Gitignore) -> bool {
    let first = rel.components().next();
    if let Some(std::path::Component::Normal(s)) = first {
        if s == std::ffi::OsStr::new(".git") { return true; }
    }
    // `matched_path_or_any_parents` walks up so a write to
    // `target/debug/x.bin` is filtered by the `target/` pattern. The
    // plain `matched` only inspects the leaf. Probe both is_dir flags
    // because directory-only patterns (`target/`) are stricter.
    ignore.matched_path_or_any_parents(rel, false).is_ignore()
        || ignore.matched_path_or_any_parents(rel, true).is_ignore()
}

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
    let ignore = Arc::new(build_ignore_matcher(&root));
    let ignore_for_event = ignore.clone();
    tracing::info!(
        target: "sprefa::fs_watcher",
        root = %root_for_event.display(),
        gitignore_loaded_globs = ignore.num_ignores(),
        "fs_watcher.gitignore.loaded",
    );
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
            // Canonicalize the event path so symlink-resolved differences
            // (macOS /private/var/... vs /var/...) don't defeat strip_prefix.
            // Fall back to the raw event path when canonicalize fails (file
            // already deleted, permission denied, …); the raw path still
            // gets probed by the gitignore matcher below.
            let ev_canon = std::fs::canonicalize(&ev.path).unwrap_or(ev.path.clone());
            // Strip the watcher root so paths align with the cursor's
            // `fs` field (which is repo-relative in normal pipelines).
            let rel_path = ev_canon
                .strip_prefix(&root_for_event)
                .ok()
                .map(|p| p.to_path_buf());
            // Probe BOTH the relative path (gitignore patterns are written
            // relative to the repo root) and the absolute path as a last
            // resort. If we have a relative form and it's gitignored, drop.
            // If strip_prefix failed entirely (event outside root, weird
            // FSEvents path), keep the event — gitignore would not be
            // authoritative on a non-repo path anyway.
            if let Some(rel) = rel_path.as_deref() {
                if should_skip(rel, &ignore_for_event) {
                    tracing::trace!(
                        target: "sprefa::fs_watcher",
                        path = %rel.display(),
                        "fs_watcher.gitignore.skip",
                    );
                    continue;
                }
            }
            paths.push(rel_path.unwrap_or(ev.path));
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

    #[tokio::test(flavor = "multi_thread")]
    async fn skips_paths_under_gitignored_target() {
        // .gitignore lists `target/`; writes under target/ must NOT
        // wake the invalidator. A sibling write outside target must.
        let dir = tempdir("ignore");
        std::fs::write(dir.join(".gitignore"), b"target/\n").unwrap();
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();

        let subject = ChangeSubject::new();
        let mut rx = subject.subscribe();

        let _guard = spawn_fs_watcher(
            Arc::from("r"),
            dir.clone(),
            subject.sender(),
            Duration::from_millis(50),
        )
        .expect("watcher spawned");

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Noise: write under target/, must be filtered out.
        std::fs::write(dir.join("target/debug/x.bin"), b"noise").unwrap();
        // Signal: write outside target/, must fire.
        std::fs::write(dir.join("a.rs"), b"fn main() {}").unwrap();

        let recv = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;
        let change = recv.expect("watcher fired").expect("subject open");
        match change {
            Change::FileBatch { paths, .. } => {
                let any_target = paths.iter().any(|p| {
                    p.components().any(|c| c.as_os_str() == std::ffi::OsStr::new("target"))
                });
                assert!(
                    !any_target,
                    "target/ paths leaked through gitignore filter: {:?}",
                    paths,
                );
            }
            other => panic!("expected FileBatch, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

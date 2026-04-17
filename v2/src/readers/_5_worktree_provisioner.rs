//! WorktreeProvisioner — lazy demand-driven `git worktree add` per (repo, rev).
//!
//! Owns a cache directory on disk and an in-memory mapping from
//! `(repo_slug, rev)` → `Arc<WtHandle>`. The first `ensure()` call for a
//! pair shells out to `git worktree add`; subsequent calls return the cached
//! Arc without any I/O.
//!
//! Single-daemon assumption (same as SqliteStore): no cross-process locking.
//!
//! Note: `tokio`'s `process` feature is not enabled in this workspace, so
//! git invocations use `std::process::Command` on a `spawn_blocking` thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::OnceCell;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A provisioned git worktree on disk.
pub struct WtHandle {
    pub path: PathBuf,
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    /// The main repo's git dir used to manage this worktree.
    pub git_dir: PathBuf,
}

/// Errors from provisioning operations.
#[derive(Debug)]
pub enum ProvErr {
    /// `git worktree add` / `remove` exited non-zero.
    Spawn(String),
    /// OS I/O failure.
    Io(std::io::Error),
    /// tokio join failure (thread panicked).
    Join(tokio::task::JoinError),
}

impl std::fmt::Display for ProvErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvErr::Spawn(msg) => write!(f, "git worktree add failed: {msg}"),
            ProvErr::Io(e)      => write!(f, "io: {e}"),
            ProvErr::Join(e)    => write!(f, "spawn_blocking panic: {e}"),
        }
    }
}

impl std::error::Error for ProvErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProvErr::Io(e)   => Some(e),
            ProvErr::Join(e) => Some(e),
            ProvErr::Spawn(_) => None,
        }
    }
}

impl From<std::io::Error> for ProvErr {
    fn from(e: std::io::Error) -> Self { ProvErr::Io(e) }
}

impl From<tokio::task::JoinError> for ProvErr {
    fn from(e: tokio::task::JoinError) -> Self { ProvErr::Join(e) }
}

// ---------------------------------------------------------------------------
// WorktreeProvisioner
// ---------------------------------------------------------------------------

pub struct WorktreeProvisioner {
    cache_root: PathBuf,
    /// `OnceCell` per key so concurrent `ensure()` calls for the same
    /// `(repo, rev)` run exactly one `git worktree add`. Losers await the
    /// winner instead of shelling a duplicate worktree.
    live: DashMap<(Arc<str>, Arc<str>), Arc<OnceCell<Arc<WtHandle>>>>,
}

impl WorktreeProvisioner {
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            cache_root,
            live: DashMap::new(),
        }
    }

    /// Lazy provisioner. First call for `(repo, rev)` runs
    /// `git worktree add <wt_dir> <rev>` with CWD = `repo_git_dir`.
    /// Subsequent calls return the cached `Arc<WtHandle>` immediately.
    ///
    /// On concurrent calls for the same key the first writer wins; the loser
    /// receives the winner's handle without duplicating the worktree on disk.
    pub async fn ensure(
        &self,
        repo: Arc<str>,
        rev:  Arc<str>,
        repo_git_dir: &Path,
    ) -> Result<Arc<WtHandle>, ProvErr> {
        let key = (repo.clone(), rev.clone());
        let cell = self
            .live
            .entry(key)
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();

        let wt_dir = self
            .cache_root
            .join(sanitize_slug(&repo))
            .join(sanitize_slug(&rev));
        let git_dir = repo_git_dir.to_path_buf();

        // `get_or_try_init` runs its future exactly once; concurrent callers
        // await the winner. On init error the cell stays empty and the next
        // caller retries.
        let handle = cell
            .get_or_try_init(|| {
                let wt_dir  = wt_dir.clone();
                let git_dir = git_dir.clone();
                let repo    = repo.clone();
                let rev     = rev.clone();
                async move {
                    let wt_dir_s  = wt_dir.clone();
                    let git_dir_s = git_dir.clone();
                    let rev_s     = rev.clone();
                    tokio::task::spawn_blocking(move || {
                        run_worktree_add_sync(&git_dir_s, &wt_dir_s, &rev_s)
                    })
                    .await??;
                    Ok::<Arc<WtHandle>, ProvErr>(Arc::new(WtHandle {
                        path: wt_dir,
                        repo,
                        rev,
                        git_dir,
                    }))
                }
            })
            .await?;

        Ok(handle.clone())
    }

    /// Remove the cached handle and run `git worktree remove --force` on disk.
    pub async fn drop_rev(
        &self,
        repo: &str,
        rev:  &str,
    ) -> Result<(), ProvErr> {
        let key: (Arc<str>, Arc<str>) = (Arc::from(repo), Arc::from(rev));
        if let Some((_, cell)) = self.live.remove(&key) {
            if let Some(handle) = cell.get() {
                let wt_path = handle.path.clone();
                let git_dir = handle.git_dir.clone();
                tokio::task::spawn_blocking(move || {
                    run_worktree_remove_sync(&git_dir, &wt_path)
                })
                .await??;
            }
        }
        Ok(())
    }

    /// Snapshot of all successfully-provisioned handles. Skips in-flight or
    /// failed cells. For observability; not a hot path.
    pub fn list(&self) -> Vec<Arc<WtHandle>> {
        self.live
            .iter()
            .filter_map(|e| e.value().get().cloned())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Sync git helpers (run inside spawn_blocking)
// ---------------------------------------------------------------------------

/// Replace path-unfriendly characters so the slug is a valid directory component.
fn sanitize_slug(s: &str) -> String {
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

fn run_worktree_add_sync(
    repo_git_dir: &Path,
    wt_dir: &Path,
    rev: &str,
) -> Result<(), ProvErr> {
    let out = std::process::Command::new("git")
        .args(["worktree", "add", "--detach", "--quiet"])
        .arg(wt_dir)
        .arg(rev)
        .current_dir(repo_git_dir)
        .output()?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(ProvErr::Spawn(stderr));
    }
    Ok(())
}

fn run_worktree_remove_sync(repo_git_dir: &Path, wt_dir: &Path) -> Result<(), ProvErr> {
    let out = std::process::Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(wt_dir)
        .current_dir(repo_git_dir)
        .output()?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(ProvErr::Spawn(stderr));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCmd;
    use tempfile::TempDir;

    /// Build a tiny git repo with one commit.
    fn make_fixture_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        StdCmd::new("git").args(["init", "-b", "main"]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["config", "user.email", "t@t.com"]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["config", "user.name", "T"]).current_dir(p).status().unwrap();
        std::fs::write(p.join("a.txt"), "hello").unwrap();
        StdCmd::new("git").args(["add", "."]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["commit", "-m", "init"]).current_dir(p).status().unwrap();
        dir
    }

    fn head_sha(repo: &Path) -> String {
        let out = StdCmd::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[tokio::test]
    async fn ensure_is_idempotent() {
        let fixture = make_fixture_repo();
        let repo_path = fixture.path();
        let sha = head_sha(repo_path);

        let cache_dir = TempDir::new().unwrap();
        let prov = WorktreeProvisioner::new(cache_dir.path().to_path_buf());

        let repo: Arc<str> = Arc::from("myorg/sprefa");
        let rev: Arc<str>  = Arc::from(sha.as_str());

        let h1 = prov.ensure(repo.clone(), rev.clone(), repo_path).await.unwrap();
        let h2 = prov.ensure(repo.clone(), rev.clone(), repo_path).await.unwrap();

        assert!(Arc::ptr_eq(&h1, &h2), "second ensure must return the same Arc");
        assert!(h1.path.exists(), "worktree path must exist on disk");
    }

    #[tokio::test]
    async fn drop_rev_removes_from_cache() {
        let fixture = make_fixture_repo();
        let repo_path = fixture.path();
        let sha = head_sha(repo_path);

        let cache_dir = TempDir::new().unwrap();
        let prov = WorktreeProvisioner::new(cache_dir.path().to_path_buf());

        let repo: Arc<str> = Arc::from("myorg/sprefa");
        let rev: Arc<str>  = Arc::from(sha.as_str());

        let h1 = prov.ensure(repo.clone(), rev.clone(), repo_path).await.unwrap();
        let wt_path = h1.path.clone();
        assert!(wt_path.exists());

        prov.drop_rev(&repo, &rev).await.unwrap();
        assert!(!wt_path.exists(), "worktree dir must be removed from disk");

        // A fresh ensure after drop must create a new WT (different Arc pointer).
        let h2 = prov.ensure(repo.clone(), rev.clone(), repo_path).await.unwrap();
        assert!(!Arc::ptr_eq(&h1, &h2), "post-drop ensure must return a fresh Arc");
        assert!(h2.path.exists());
    }
}

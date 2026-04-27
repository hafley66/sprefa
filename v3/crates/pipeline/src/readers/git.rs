//! GitFileSource — `FileSource` impl backed by libgit2 (`git2` crate).
//!
//! Serves files + bytes for an immutable `(repo_root, commit_oid)` pair.
//! Unlike `DiskFileSource`, the rev label is load-bearing: the commit OID
//! is resolved once at construction (`HEAD`, branch, tag, raw SHA, or
//! `refs/...`), and every subsequent read targets that commit's tree.
//!
//! Dispatch is the caller's job (`server::run::wrap_seed_source` and
//! `server::backend.rs` hover wiring): rev `wt` → `DiskFileSource`,
//! anything else → `GitFileSource`. `:wt` and `wt` are accepted
//! interchangeably as the worktree label.
//!
//! # Concurrency
//!
//! `git2::Repository` is `Send + !Sync`, so the handle sits behind a
//! `Mutex`. Per-process there is at most one open handle per
//! `(repo_root, oid)` pair (each seed builds its own source).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::FileSource;

pub struct GitFileSource {
    repo_root: PathBuf,
    /// Resolved commit OID. The user-facing rev string lives on the
    /// cursor; the source operates only on the resolved OID.
    oid: git2::Oid,
    /// Tree OID of the commit. Cached so `files()` and every
    /// `file_bytes()` skip the `find_commit -> tree_id` walk.
    tree_oid: git2::Oid,
    repo: Mutex<git2::Repository>,
}

impl GitFileSource {
    /// Open `repo_root` and resolve `rev` to a commit OID. Accepts:
    /// - `HEAD` / `:HEAD`
    /// - branch / tag short names (`main`, `v1.2.3`)
    /// - `refs/heads/...`, `refs/tags/...`, `refs/remotes/...`
    /// - 40-hex SHA or short prefix (libgit2 disambiguates)
    pub fn open(repo_root: PathBuf, rev: &str) -> Result<Self, git2::Error> {
        let repo = git2::Repository::open(&repo_root)?;
        let oid = resolve_rev(&repo, rev)?;
        let tree_oid = repo.find_commit(oid)?.tree_id();
        Ok(Self {
            repo_root,
            oid,
            tree_oid,
            repo: Mutex::new(repo),
        })
    }

    pub fn oid(&self) -> git2::Oid { self.oid }
    pub fn root(&self) -> &Path { &self.repo_root }
}

/// Resolve a user-facing rev string to a commit OID. Strips a leading
/// `:` (sprefa surface syntax) before delegating to `revparse_single`.
pub fn resolve_rev(repo: &git2::Repository, rev: &str) -> Result<git2::Oid, git2::Error> {
    let rev = rev.strip_prefix(':').unwrap_or(rev);
    let obj = repo.revparse_single(rev)?;
    let commit = obj.peel_to_commit()?;
    Ok(commit.id())
}

impl FileSource for GitFileSource {
    fn files(&self, _repo: &str, rev: &str) -> Vec<Arc<Path>> {
        let _t0 = std::time::Instant::now();
        let guard = self.repo.lock().unwrap();
        let mut out = Vec::new();
        if let Ok(tree) = guard.find_tree(self.tree_oid) {
            let _ = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    let name = entry.name().unwrap_or("");
                    let full = if root.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{}{}", root, name)
                    };
                    out.push(Arc::<Path>::from(Path::new(&full)));
                }
                git2::TreeWalkResult::Ok
            });
        }
        tracing::info!(
            target: "sprefa::reader",
            root = %self.repo_root.display(),
            rev = %rev,
            oid = %self.oid,
            files = out.len(),
            elapsed_ms = _t0.elapsed().as_millis() as u64,
            "GitFileSource.files"
        );
        out
    }

    fn file_bytes(&self, _repo: &str, _rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        let (bytes, _) = self.read_blob(path)?;
        Some(bytes)
    }

    fn file_bytes_with_hash(
        &self,
        _repo: &str,
        _rev:  &str,
        path: &Path,
    ) -> Option<(Arc<[u8]>, Option<Arc<[u8; 32]>>)> {
        let (bytes, oid) = self.read_blob(path)?;
        let mut hash = [0u8; 32];
        hash[..20].copy_from_slice(oid.as_bytes());
        Some((bytes, Some(Arc::new(hash))))
    }
}

impl GitFileSource {
    fn read_blob(&self, path: &Path) -> Option<(Arc<[u8]>, git2::Oid)> {
        let _t0 = std::time::Instant::now();
        let guard = self.repo.lock().unwrap();
        let tree = guard.find_tree(self.tree_oid).ok()?;
        let entry = tree.get_path(path).ok()?;
        let oid = entry.id();
        let obj = entry.to_object(&guard).ok()?;
        let blob = obj.peel_to_blob().ok()?;
        let bytes: Arc<[u8]> = Arc::from(blob.content().to_vec());
        tracing::trace!(
            target: "sprefa::reader",
            path = %path.display(),
            oid = %oid,
            bytes = bytes.len(),
            elapsed_us = _t0.elapsed().as_micros() as u64,
            "GitFileSource.file_bytes"
        );
        Some((bytes, oid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(p)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t.com")
                .status()
                .expect("git");
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name", "t"]);
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("src/main.rs"), b"fn main() {}").unwrap();
        std::fs::write(p.join("README.md"), b"v1").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "init"]);
        // Second commit; mutate README so HEAD vs HEAD~1 differ.
        std::fs::write(p.join("README.md"), b"v2").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "v2"]);
        // Tag the first commit so we can test tag resolution.
        run(&["tag", "v1", "HEAD~1"]);
        tmp
    }

    #[test]
    fn resolves_head_and_lists_files() {
        let tmp = init_fixture();
        let src = GitFileSource::open(tmp.path().to_path_buf(), "HEAD").unwrap();
        let mut files: Vec<String> = src.files("r", "HEAD")
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(files, vec!["README.md", "src/main.rs"]);
    }

    #[test]
    fn rev_label_targets_distinct_commits() {
        let tmp = init_fixture();
        let head = GitFileSource::open(tmp.path().to_path_buf(), "HEAD").unwrap();
        let v1   = GitFileSource::open(tmp.path().to_path_buf(), "v1").unwrap();

        let head_bytes = head.file_bytes("r", "HEAD", Path::new("README.md")).unwrap();
        let v1_bytes   = v1.file_bytes("r", "v1", Path::new("README.md")).unwrap();
        assert_eq!(&*head_bytes, b"v2");
        assert_eq!(&*v1_bytes,   b"v1");
    }

    #[test]
    fn colon_prefix_is_stripped() {
        let tmp = init_fixture();
        let src = GitFileSource::open(tmp.path().to_path_buf(), ":HEAD").unwrap();
        let bytes = src.file_bytes("r", ":HEAD", Path::new("README.md")).unwrap();
        assert_eq!(&*bytes, b"v2");
    }

    #[test]
    fn file_bytes_with_hash_returns_blob_oid() {
        let tmp = init_fixture();
        let src = GitFileSource::open(tmp.path().to_path_buf(), "HEAD").unwrap();
        let (_bytes, hash) = src
            .file_bytes_with_hash("r", "HEAD", Path::new("README.md"))
            .unwrap();
        let hash = hash.expect("git source supplies blob oid");
        // Trailing 12 bytes are zero-padding (SHA-1 is 20 bytes).
        assert_eq!(&hash[20..], &[0u8; 12]);
    }
}

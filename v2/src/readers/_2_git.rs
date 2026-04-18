//! GitBlobReader — reads files and bytes from git object DB.
//! No working-tree checkout required; works with bare clones, mirrors,
//! and user checkouts alike.

use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;
use tracing::{info_span, Instrument};

use crate::_0_types::FilePath;
use crate::_2_config::Config;
use crate::_3_reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::_16_pattern::CompiledPattern;
use super::_1_locator::CheckoutLocator;
use super::_5_worktree_provisioner::WorktreeProvisioner;
use super::_6_path_index::PathIndex;

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

pub struct GitBlobReader {
    locator: Arc<dyn CheckoutLocator>,
    /// Lazily opened repo handles. git2::Repository is Send but not Sync;
    /// each entry is behind its own Mutex for &self method access.
    repos:   Mutex<HashMap<String, Arc<Mutex<git2::Repository>>>>,
    /// Tree-oid cache per (repo, rev). Without this, every bytes() call
    /// reruns `revparse_single` + `peel_to_commit` — N-files sequential
    /// revparses on a single repo. With it, one revparse per (repo,rev);
    /// later calls take `find_tree(oid)` directly.
    trees:   Mutex<HashMap<(String, String), git2::Oid>>,
    config:  Arc<Config>,
    /// Optional path index: (repo, rev) → [(path, blob_oid)] cache.
    /// None = disabled; phase 1 and phase 2 are skipped.
    pub provisioner: Option<Arc<WorktreeProvisioner>>,
    /// Optional worktree provisioner for phase 2 WT materialization.
    /// None = phase 2 is skipped; falls straight to phase 3 libgit2 walk.
    pub path_index:  Option<Arc<dyn PathIndex>>,
}

impl GitBlobReader {
    /// Baseline ctor — path_index and provisioner both None.
    /// All existing callers remain valid without change.
    pub fn new(locator: Arc<dyn CheckoutLocator>, config: Arc<Config>) -> Self {
        Self::new_with_index(locator, config, None, None)
    }

    /// Full ctor used by the server when a path index and provisioner are available.
    pub fn new_with_index(
        locator:     Arc<dyn CheckoutLocator>,
        config:      Arc<Config>,
        provisioner: Option<Arc<WorktreeProvisioner>>,
        path_index:  Option<Arc<dyn PathIndex>>,
    ) -> Self {
        Self {
            locator,
            repos:  Mutex::new(HashMap::new()),
            trees:  Mutex::new(HashMap::new()),
            config,
            provisioner,
            path_index,
        }
    }

    fn open_repo(&self, slug: &str) -> Option<Arc<Mutex<git2::Repository>>> {
        let mut cache = self.repos.lock().unwrap();
        if let Some(r) = cache.get(slug) {
            return Some(r.clone());
        }
        let path = self.locator.locate(slug, "")?;
        let repo = git2::Repository::open(&path).ok()?;
        let entry = Arc::new(Mutex::new(repo));
        cache.insert(slug.to_string(), entry.clone());
        Some(entry)
    }

    /// Tree Oid for (repo, rev). Cached after first revparse.
    fn tree_oid(&self, repo_slug: &str, rev: &str, repo: &git2::Repository)
        -> Result<git2::Oid, git2::Error>
    {
        let _s = info_span!("reader.git.tree_oid", repo = %repo_slug, rev = %rev).entered();
        {
            let g = self.trees.lock().unwrap();
            if let Some(oid) = g.get(&(repo_slug.to_string(), rev.to_string())) {
                return Ok(*oid);
            }
        }
        let _revparse = info_span!("reader.git.revparse").entered();
        let obj = repo.revparse_single(rev)?;
        let commit = obj.peel_to_commit()?;
        let oid = commit.tree_id();
        drop(_revparse);
        self.trees.lock().unwrap()
            .insert((repo_slug.to_string(), rev.to_string()), oid);
        Ok(oid)
    }

    fn walk_tree(tree: &git2::Tree, repo: &git2::Repository, pattern: &CompiledPattern) -> Vec<FilePath> {
        let _s = info_span!("reader.git.tree_walk").entered();
        let mut paths = Vec::new();
        let _ = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                let name = entry.name().unwrap_or("");
                let full = if root.is_empty() {
                    name.to_owned()
                } else {
                    format!("{}{}", root, name)
                };
                if pattern.is_match(&full) {
                    paths.push(FilePath(Arc::from(Path::new(&full))));
                }
            }
            git2::TreeWalkResult::Ok
        });
        let _ = repo; // suppress unused warning; may need repo for subtree walks later
        paths
    }

    /// Phase 3 fallback: libgit2 tree walk. Returns empty on any git error.
    fn files_libgit2(&self, repo: &str, rev: &str, pattern: &CompiledPattern) -> Vec<FilePath> {
        let _s = info_span!("reader.git.files_libgit2", repo = %repo, rev = %rev).entered();
        let Some(repo_arc) = self.open_repo(repo) else {
            return vec![];
        };
        let guard = repo_arc.lock().unwrap();
        match self.tree_oid(repo, rev, &guard) {
            Ok(oid) => {
                let _s2 = info_span!("reader.git.find_tree").entered();
                guard.find_tree(oid)
                    .map(|tree| { drop(_s2); Self::walk_tree(&tree, &guard, pattern) })
                    .unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    }
}

fn once_val<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::once(async move { v }))
}

/// Walk a git tree via `git ls-tree -r -z <rev>` from the checkout path.
/// Returns (FilePath, blob_oid) for every blob. One subprocess call, no
/// per-entry I/O. Preferred over ignore::WalkBuilder because it supplies oids
/// alongside paths in a single pass.
fn walk_wt_with_ls_tree(
    checkout_path: std::path::PathBuf,
    rev: String,
) -> Vec<(FilePath, [u8; 20])> {
    let _s = info_span!("reader.git.ls_tree", rev = %rev).entered();
    // Output format per entry (null-delimited with -z):
    //   "<mode> <type> <oid>\t<path>\0"
    let _sp = info_span!("reader.git.ls_tree.spawn").entered();
    let out = match std::process::Command::new("git")
        .args(["ls-tree", "-r", "-z", &rev])
        .current_dir(&checkout_path)
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };
    drop(_sp);

    let _pp = info_span!("reader.git.ls_tree.parse", bytes = out.len()).entered();
    let mut result = Vec::new();
    // Split on NUL bytes; each element is one entry (may be empty at end).
    for entry in out.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        // tab separates the "<mode> <type> <oid>" header from the path.
        let Some(tab) = entry.iter().position(|&b| b == b'\t') else { continue };
        let header = &entry[..tab];
        let path   = match std::str::from_utf8(&entry[tab + 1..]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // header = "<mode> blob <40-hex-oid>"
        let parts: Vec<&[u8]> = header.splitn(3, |&b| b == b' ').collect();
        if parts.len() != 3 || parts[1] != b"blob" {
            continue; // skip trees, commits, etc.
        }
        let oid_hex = match std::str::from_utf8(parts[2]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if oid_hex.len() != 40 {
            continue;
        }
        let mut oid = [0u8; 20];
        let ok = oid_hex.as_bytes()
            .chunks(2)
            .zip(oid.iter_mut())
            .all(|(pair, byte)| {
                let hi = (pair[0] as char).to_digit(16);
                let lo = (pair[1] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => { *byte = (h * 16 + l) as u8; true }
                    _ => false,
                }
            });
        if !ok { continue; }
        result.push((FilePath(Arc::from(Path::new(path))), oid));
    }
    result
}

impl Reader for GitBlobReader {
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern) -> BoxStream<'static, Vec<FilePath>> {
        // Sync fast path: no index wired → reuse self's repo/tree caches.
        // Without this, the async block below rebuilds a fresh GitBlobReader
        // per call with empty caches, redoing revparse on every files() call.
        if self.path_index.is_none() {
            let _s = info_span!("reader.files", repo = %repo, rev = %rev, path = "libgit2").entered();
            let hits = self.files_libgit2(repo, rev, pattern);
            return once_val(hits);
        }

        // Clone Arcs so the async block is 'static.
        let repo_s      = repo.to_owned();
        let rev_s       = rev.to_owned();
        let pattern     = pattern.clone();
        let path_index  = self.path_index.clone();
        let provisioner = self.provisioner.clone();
        let locator     = self.locator.clone();
        let config      = self.config.clone();

        // Phases 1 and 2 require .await (PathIndex is async_trait).
        // Wrap in stream::once so the Reader trait signature is unchanged.
        let outer_span = info_span!("reader.files", repo = %repo_s, rev = %rev_s);
        Box::pin(stream::once(async move {
            let timing = std::env::var_os("SPREFA_TIMING").is_some();
            // ---------------------------------------------------------------
            // Phase 1 — PathIndex hit: exactly one files_at call.
            // ---------------------------------------------------------------
            if let Some(ref idx) = path_index {
                let t0 = std::time::Instant::now();
                match idx.files_at(&repo_s, &rev_s, &pattern)
                    .instrument(info_span!("reader.files.phase1.files_at"))
                    .await {
                    Ok(Some(hits)) => {
                        if timing { eprintln!(
                            "[timing] path_index.hit {:>5}ms repo={} rev={} n={}",
                            t0.elapsed().as_millis(), repo_s, rev_s, hits.len()); }
                        // Adopt on-disk WT into the provisioner's in-mem map so
                        // bytes() can take the filesystem fast path. ensure()
                        // is idempotent: if the WT dir exists, it skips the
                        // shell call and just registers the handle.
                        if let Some(ref prov) = provisioner {
                            let checkout = locator.locate(&repo_s, &rev_s)
                                .or_else(|| locator.locate(&repo_s, "HEAD"));
                            if let Some(cp) = checkout {
                                let t_adopt = std::time::Instant::now();
                                let repo_arc: Arc<str> = Arc::from(repo_s.as_str());
                                let rev_arc:  Arc<str> = Arc::from(rev_s.as_str());
                                if let Err(e) = prov.ensure(repo_arc, rev_arc, &cp)
                                    .instrument(info_span!("reader.files.phase1.adopt_ensure"))
                                    .await {
                                    eprintln!("[worktree] adopt error: {e}");
                                } else if timing {
                                    eprintln!(
                                        "[timing] wt.adopt      {:>5}ms repo={} rev={}",
                                        t_adopt.elapsed().as_millis(), repo_s, rev_s);
                                }
                            }
                        }
                        return hits;
                    }
                    Ok(None)       => {
                        if timing { eprintln!(
                            "[timing] path_index.miss {:>5}ms repo={} rev={}",
                            t0.elapsed().as_millis(), repo_s, rev_s); }
                    }
                    Err(e)         => {
                        // index error — skip phases 1+2, fall straight to phase 3
                        eprintln!("[path_index] files_at error: {e}");
                        let reader = GitBlobReader::new(locator, config);
                        return reader.files_libgit2(&repo_s, &rev_s, &pattern);
                    }
                }
            }

            // ---------------------------------------------------------------
            // Phase 2 — WT materialize + ls-tree + bulk upsert + re-query.
            //   Exactly: one ensure, one ls-tree, one upsert_rev, one files_at.
            // ---------------------------------------------------------------
            if let (Some(ref idx), Some(ref prov)) = (&path_index, &provisioner) {
                // Resolve checkout path from locator; try rev first, then "HEAD".
                // ls-tree uses the explicit rev arg, not the WT checkout state.
                let checkout_path = locator.locate(&repo_s, &rev_s)
                    .or_else(|| locator.locate(&repo_s, "HEAD"));

                if let Some(checkout_path) = checkout_path {
                    let repo_arc: Arc<str> = Arc::from(repo_s.as_str());
                    let rev_arc:  Arc<str> = Arc::from(rev_s.as_str());

                    let t_ensure = std::time::Instant::now();
                    match prov.ensure(repo_arc, rev_arc, &checkout_path)
                        .instrument(info_span!("reader.files.phase2.ensure"))
                        .await {
                        Ok(_wt) => {
                            let d_ensure = t_ensure.elapsed();
                            // Walk via ls-tree: one subprocess, oids alongside paths.
                            let cp2  = checkout_path.clone();
                            let rev2 = rev_s.clone();
                            let t_ls = std::time::Instant::now();
                            let entries: Vec<(FilePath, [u8; 20])> =
                                tokio::task::spawn_blocking(move || {
                                    walk_wt_with_ls_tree(cp2, rev2)
                                })
                                .instrument(info_span!("reader.files.phase2.ls_tree_join"))
                                .await
                                .unwrap_or_default();
                            let d_ls = t_ls.elapsed();

                            // Bulk upsert — one transaction for all entries.
                            let t_up = std::time::Instant::now();
                            if let Err(e) = idx.upsert_rev(&repo_s, &rev_s, &entries)
                                .instrument(info_span!("reader.files.phase2.upsert_rev", n = entries.len()))
                                .await {
                                eprintln!("[path_index] upsert_rev error: {e}");
                            }
                            let d_up = t_up.elapsed();
                            if timing { eprintln!(
                                "[timing] phase2 repo={} rev={} ensure={}ms ls-tree={}ms upsert={}ms rows={}",
                                repo_s, rev_s,
                                d_ensure.as_millis(), d_ls.as_millis(), d_up.as_millis(),
                                entries.len()); }

                            // Re-query index: exactly one files_at call.
                            match idx.files_at(&repo_s, &rev_s, &pattern)
                                .instrument(info_span!("reader.files.phase2.requery_files_at"))
                                .await {
                                Ok(Some(hits)) => return hits,
                                Ok(None) => {
                                    // Pattern matched nothing; filter in-process.
                                    return entries.into_iter()
                                        .filter(|(fp, _)| {
                                            pattern.is_match(&fp.0.to_string_lossy())
                                        })
                                        .map(|(fp, _)| fp)
                                        .collect();
                                }
                                Err(e) => {
                                    eprintln!("[path_index] post-upsert files_at error: {e}");
                                    // Fall through to phase 3.
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[worktree] ensure error: {e}");
                            // Fall through to phase 3.
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // Phase 3 — libgit2 tree walk (unchanged from original).
            //   Runs when path_index/provisioner are None, or phases 1/2 fail.
            // ---------------------------------------------------------------
            let t3 = std::time::Instant::now();
            let reader = GitBlobReader::new(locator, config);
            let hits = reader.files_libgit2(&repo_s, &rev_s, &pattern);
            if timing { eprintln!(
                "[timing] phase3.libgit2 {:>5}ms repo={} rev={} n={}",
                t3.elapsed().as_millis(), repo_s, rev_s, hits.len()); }
            hits
        }.instrument(outer_span)))
    }

    fn bytes(&self, repo: &str, rev: &str, fs: &FilePath) -> BoxStream<'static, Bytes> {
        let _span = info_span!(
            "reader.bytes",
            repo = %repo, rev = %rev,
            fp = %fs.0.display(),
            path = tracing::field::Empty,
        ).entered();

        // Fast path: if a worktree is already provisioned for (repo, rev),
        // read the file from disk. files() ensured this before any bytes()
        // call, so on warm paths we never touch libgit2 for content reads.
        if let Some(prov) = self.provisioner.as_ref() {
            if let Some(wt) = prov.get(repo, rev) {
                let full = wt.path.join(fs.0.as_ref());
                if let Ok(data) = std::fs::read(&full) {
                    _span.record("path", "wt");
                    return once_val(Bytes::from(data));
                }
            }
        }
        _span.record("path", "libgit2");

        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(Bytes::new());
        };
        let rel = fs.0.to_string_lossy().into_owned();
        let result = {
            let _lg = info_span!("reader.git.bytes.peel").entered();
            let guard = repo_arc.lock().unwrap();
            (|| -> Option<Bytes> {
                let oid = self.tree_oid(repo, rev, &guard).ok()?;
                let tree = guard.find_tree(oid).ok()?;
                let entry = tree.get_path(Path::new(&rel)).ok()?;
                let obj = entry.to_object(&guard).ok()?;
                let blob = obj.peel_to_blob().ok()?;
                Some(Bytes::copy_from_slice(blob.content()))
            })().unwrap_or_default()
        };
        once_val(result)
    }

    fn bytes_range(&self, repo: &str, rev: &str, fs: &FilePath, range: Range<usize>)
        -> BoxStream<'static, Bytes>
    {
        let _span = info_span!(
            "reader.bytes_range",
            repo = %repo, rev = %rev,
            fp = %fs.0.display(),
            start = range.start, end = range.end,
            path = tracing::field::Empty,
        ).entered();
        if let Some(prov) = self.provisioner.as_ref() {
            if let Some(wt) = prov.get(repo, rev) {
                let full = wt.path.join(fs.0.as_ref());
                if let Ok(data) = std::fs::read(&full) {
                    _span.record("path", "wt");
                    let start = range.start.min(data.len());
                    let end   = range.end.min(data.len());
                    return once_val(Bytes::copy_from_slice(&data[start..end]));
                }
            }
        }
        _span.record("path", "libgit2");

        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(Bytes::new());
        };
        let rel = fs.0.to_string_lossy().into_owned();
        let result = {
            let _lg = info_span!("reader.git.bytes_range.peel").entered();
            let guard = repo_arc.lock().unwrap();
            (|| -> Option<Bytes> {
                let oid = self.tree_oid(repo, rev, &guard).ok()?;
                let tree = guard.find_tree(oid).ok()?;
                let entry = tree.get_path(Path::new(&rel)).ok()?;
                let obj = entry.to_object(&guard).ok()?;
                let blob = obj.peel_to_blob().ok()?;
                let content = blob.content();
                let start = range.start.min(content.len());
                let end   = range.end.min(content.len());
                Some(Bytes::copy_from_slice(&content[start..end]))
            })().unwrap_or_default()
        };
        once_val(result)
    }

    fn blob_oid(&self, repo: &str, rev: &str, fs: &FilePath)
        -> BoxStream<'static, Option<[u8; 20]>>
    {
        let _span = info_span!("reader.blob_oid", repo = %repo, rev = %rev, fp = %fs.0.display()).entered();
        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(None);
        };
        let rel = fs.0.to_string_lossy().into_owned();
        let result = {
            let _lg = info_span!("reader.git.blob_oid.peel").entered();
            let guard = repo_arc.lock().unwrap();
            (|| -> Option<[u8; 20]> {
                let oid = self.tree_oid(repo, rev, &guard).ok()?;
                let tree = guard.find_tree(oid).ok()?;
                let entry = tree.get_path(Path::new(&rel)).ok()?;
                let id = entry.id();
                let mut out = [0u8; 20];
                out.copy_from_slice(id.as_bytes());
                Some(out)
            })()
        };
        once_val(result)
    }

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>> {
        once_val(self.locator.repos())
    }

    fn revs(&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>> {
        once_val(self.locator.revs(repo))
    }

    fn parsed(&self, _: &str, _: &str, _: &FilePath, _: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>
    { unimplemented!("stage C+: parsed trees") }

    fn cross_ref(&self, _: &str, _: &str, _: &str, _: &str)
        -> BoxStream<'static, Vec<CrossRefHit>>
    { unimplemented!() }

    fn unscanned(&self, _: &str, _: &str, _: ScanKind, _: bool)
        -> BoxStream<'static, Vec<ScanCombo>>
    { unimplemented!() }

    fn violations(&self, _: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>>
    { unimplemented!() }

    fn run_visited(&self, _: u64, _: u64, _: u64) -> BoxStream<'static, bool>
    { unimplemented!() }

    fn config(&self) -> BoxStream<'static, Arc<Config>> {
        once_val(self.config.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use futures_util::stream::StreamExt as _;

    use crate::_16_pattern::CompiledPattern;
    use crate::_2_config::Config;
    use super::super::_1_locator::InMemoryLocator;
    use super::super::_6_path_index::SqlxPathIndex;

    fn pat(s: &str) -> CompiledPattern {
        CompiledPattern::compile(s).expect("valid pattern")
    }

    fn fp(s: &str) -> FilePath {
        FilePath(Arc::from(Path::new(s)))
    }

    /// A `CheckoutLocator` that panics on `locate()`. Used to verify phase 1
    /// returns from the index without touching git at all.
    struct PanicLocator;

    impl CheckoutLocator for PanicLocator {
        fn locate(&self, _repo: &str, _rev: &str) -> Option<PathBuf> {
            panic!("locate() must not be called when phase 1 hits the index")
        }
        fn repos(&self)                  -> Vec<Arc<str>> { vec![] }
        fn revs(&self, _: &str)          -> Vec<Arc<str>> { vec![] }
    }

    /// Phase 1 hit: pre-populate a SqlxPathIndex, construct GitBlobReader with
    /// it and a PanicLocator, call files() — must return index results without
    /// ever reaching the git2 locator.
    #[tokio::test]
    async fn files_phase1_hit_skips_git() {
        let idx = SqlxPathIndex::open_memory().await.unwrap();
        idx.upsert_rev("myrepo", "abc", &[
            (fp("src/main.rs"),  [0u8; 20]),
            (fp("src/lib.rs"),   [0u8; 20]),
            (fp("build.rs"),     [0u8; 20]),
        ]).await.unwrap();

        let locator: Arc<dyn CheckoutLocator> = Arc::new(PanicLocator);
        let config  = Arc::new(Config::test_default());
        let reader  = GitBlobReader::new_with_index(
            locator,
            config,
            None,
            Some(idx as Arc<dyn PathIndex>),
        );

        let mut stream = reader.files("myrepo", "abc", &pat("src/*.rs"));
        let files = stream.next().await.unwrap_or_default();

        let mut names: Vec<String> = files.iter()
            .map(|fp| fp.0.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["src/lib.rs", "src/main.rs"]);
    }

    /// Phase 3 fallback: path_index and provisioner are both None.
    /// Construct a real git repo fixture, verify files() returns blobs
    /// via the libgit2 walk unchanged from the original behavior.
    #[test]
    fn files_phase3_fallback() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let p   = tmp.path();

        // Minimal fixture repo.
        let run = |args: &[&str]| {
            Command::new("git").args(args).current_dir(p)
                .env("GIT_AUTHOR_NAME",    "t").env("GIT_AUTHOR_EMAIL", "t@t.com")
                .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t.com")
                .status().expect("git");
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "t@t.com"]);
        run(&["config", "user.name",  "t"]);
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("src/main.rs"), b"fn main() {}").unwrap();
        std::fs::write(p.join("build.rs"),    b"fn main() {}").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "init"]);

        let slug = "test/repo";
        let locator = Arc::new(
            InMemoryLocator::new().with_repo(slug, p.to_path_buf(), &["HEAD"])
        );
        let config = Arc::new(Config::test_default());
        // No path_index, no provisioner — pure phase 3.
        let reader = GitBlobReader::new(locator, config);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all().build().unwrap();
        let files = rt.block_on(async {
            let mut s = reader.files(slug, "HEAD", &pat("src/*.rs"));
            s.next().await.unwrap_or_default()
        });

        let mut names: Vec<String> = files.iter()
            .map(|fp| fp.0.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["src/main.rs"]);
    }

    /// Phase 2 end-to-end would require shelling git worktree add — skipped here.
    /// Mark intent for 6g bench integration.
    #[tokio::test]
    #[ignore]
    async fn files_phase2_wt_materialize_and_index() {
        // 6g: construct a real fixture repo + WorktreeProvisioner + SqlxPathIndex,
        // call files() on a cold index, assert WT is populated and index is warm.
    }
}

//! Receive-side filter shared by the daemon watcher and the pre-daemon
//! `--watch` loop. The recursive file watcher wakes on *every* write under a
//! served root: `target/`, `node_modules/`, gitignored build output, and the
//! `.git/objects` churn a single `git status` or `git checkout` produces. The
//! scan corpus, by contrast, honors `.gitignore` and prunes `.git`
//! (`engine::enumerate_with_hash`). `WatchGate` mirrors that include decision so
//! the watcher only ticks for files the engine could actually scan, plus the
//! narrow git paths (`HEAD`/`packed-refs`/`refs/`) whose change moves a ref.
//!
//! Divergence from the scan walk (documented, safe): the walk is a
//! `WalkBuilder` that layers *nested* `.gitignore` files it descends through;
//! `GitignoreBuilder` here loads only each root's top-level `.gitignore` +
//! `.git/info/exclude` + the global gitignore. A file the gate keeps but the
//! scan drops is harmless (`tick_paths` drops it again at its glob filter). The
//! dangerous direction is a false *drop* — a single-root `Gitignore` cannot
//! over-match relative to the walk's own root-level rules, so this stays safe.
//! The matchers are built once and rebuilt only on daemon restart; editing a
//! `.gitignore` does not re-derive them (a known, accepted limitation).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// One watched repo root plus its root-level gitignore matcher. `prefixes`
/// holds both the as-given root and its canonical form, because notify's event
/// paths are canonical on macOS (FSEvents resolves symlinks) but echo the
/// as-watched path on Linux; matching either keeps both platforms correct.
struct GateRoot {
    prefixes: Vec<PathBuf>,
    ignore: Gitignore,
}

/// The receive-side include gate. One instance per watch loop, owned on the
/// watcher thread; mutated only by `add_root` when a tick pulls a new repo.
pub struct WatchGate {
    roots: Vec<GateRoot>,
    /// Canonical `.git` directory paths. A path inside one of these is git
    /// noise UNLESS it is under one of the narrow ref paths below.
    git_dirs: Vec<PathBuf>,
    /// Canonical narrow git paths (`HEAD`, `packed-refs`, `refs/`) whose change
    /// means a ref moved. Kept and routed to `on_git_event`.
    git_paths: Vec<PathBuf>,
}

impl WatchGate {
    /// Build a gate over the eager repo set. Each root contributes a matcher
    /// from its `.gitignore`, `.git/info/exclude`, and the global gitignore.
    pub fn new(roots: &[PathBuf]) -> Self {
        let mut gate = WatchGate { roots: Vec::new(), git_dirs: Vec::new(), git_paths: Vec::new() };
        for root in roots {
            gate.add_root(root);
        }
        gate
    }

    /// Register another root (a dynamically-pulled repo). Idempotent on the
    /// canonicalized root.
    pub fn add_root(&mut self, root: &Path) {
        let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if self.roots.iter().any(|r| r.prefixes.contains(&canon)) {
            return;
        }
        let mut prefixes = vec![root.to_path_buf()];
        if canon != root {
            prefixes.push(canon.clone());
        }
        let mut builder = GitignoreBuilder::new(&canon);
        // Root-level ignore sources, mirroring the scan walk's root layer.
        let _ = builder.add(canon.join(".gitignore"));
        let _ = builder.add(canon.join(".git/info/exclude"));
        if let Ok(global) = std::env::var("HOME") {
            let _ = builder.add(Path::new(&global).join(".config/git/ignore"));
        }
        let ignore = builder.build().unwrap_or_else(|_| Gitignore::empty());
        self.roots.push(GateRoot { prefixes, ignore });
    }

    /// Register a `.git` directory and its narrow ref paths. Events under
    /// `git_dir` but not under a narrow ref path are dropped (`.git/objects`
    /// churn); events under a narrow ref path are kept and `touches_git`-routed.
    pub fn add_git_dir(&mut self, git_dir: &Path) {
        let canon = git_dir.canonicalize().unwrap_or_else(|_| git_dir.to_path_buf());
        // Both forms (as-given + canonical), same platform reason as GateRoot.
        for form in [git_dir.to_path_buf(), canon] {
            if !self.git_dirs.contains(&form) {
                for sub in ["HEAD", "packed-refs", "refs"] {
                    self.git_paths.push(form.join(sub));
                }
                self.git_dirs.push(form);
            }
        }
    }

    /// The narrow watch targets for a git dir: the git dir itself NonRecursive
    /// (delivers `HEAD`/`packed-refs`/top-level files — but NOT the `objects/`
    /// churn two levels down) and `refs/` Recursive (small). NonRecursive on the
    /// git dir is robust to `packed-refs` being created later (by `git gc`),
    /// which a fixed file-level watch would miss. `(path, recursive)`.
    pub fn git_watch_targets(&self, git_dir: &Path) -> Vec<(PathBuf, bool)> {
        let canon = git_dir.canonicalize().unwrap_or_else(|_| git_dir.to_path_buf());
        vec![
            (canon.clone(), false),      // git dir NonRecursive
            (canon.join("refs"), true),  // refs/ Recursive
        ]
    }

    /// Retain only paths the engine could scan (or a narrow ref path),
    /// de-duplicated. See the module header for the include decision.
    pub fn filter(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut kept: Vec<PathBuf> = Vec::new();
        for path in paths {
            if is_daemon_internal(&path) {
                continue;
            }
            // Git: keep only the narrow ref paths; drop all other `.git` churn.
            if self.git_dirs.iter().any(|g| path.starts_with(g)) {
                if self.git_paths.iter().any(|gp| path.starts_with(gp) || &path == gp)
                    && seen.insert(path.clone()) {
                    kept.push(path);
                }
                continue;
            }
            // Under a known root: apply that root's gitignore matcher, relative
            // to whichever prefix (as-given or canonical) matched.
            if let Some((gr, prefix)) = self.roots.iter()
                .find_map(|r| r.prefixes.iter().find(|p| path.starts_with(p)).map(|p| (r, p))) {
                let is_dir = path.is_dir();
                if let Ok(rel) = path.strip_prefix(prefix) {
                    // `_or_any_parents` so a file under an ignored dir (`target/`
                    // matching `target/out`) is dropped, like git itself.
                    if gr.ignore.matched_path_or_any_parents(rel, is_dir).is_ignore() {
                        continue;
                    }
                }
                // Nested repo (submodule) pruning, mirroring the scan walker's
                // `enumerate_with_hash`: an ancestor dir strictly between this
                // path and the root that itself owns a `.git` entry (dir or
                // file) is a foreign repo, and its contents are not part of
                // this root's corpus even though nothing gitignores them.
                // Bounded by path depth; no memoization needed (paths under a
                // deep nested repo are rare and events already arrive
                // debounced by the quiet-period window upstream).
                if is_under_nested_repo(&path, prefix) {
                    continue;
                }
                if seen.insert(path.clone()) {
                    kept.push(path);
                }
                continue;
            }
            // Out of every known root and not git: an edit to a config file or
            // another repo. Keep it — the tick's own strip_prefix will drop it
            // if it truly matters to nothing, but silently swallowing here would
            // hide a real full-tick trigger.
            if seen.insert(path.clone()) {
                kept.push(path);
            }
        }
        kept
    }

    /// True if any kept path is under a narrow git ref path.
    pub fn touches_git(&self, kept: &[PathBuf]) -> bool {
        !self.git_paths.is_empty()
            && kept.iter().any(|p| self.git_paths.iter().any(|gp| p.starts_with(gp) || p == gp))
    }
}

/// True if some ancestor of `path`, strictly between `path` and `root`
/// (exclusive of `root` itself — the root's own `.git` is handled by the
/// caller's git-dir arm), owns a `.git` entry. Walks parent dirs up from
/// `path`'s immediate parent; a stat per ancestor, bounded by path depth.
fn is_under_nested_repo(path: &Path, root: &Path) -> bool {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) {
            break;
        }
        if d.join(".git").exists() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// True for the daemon's own bookkeeping files: the sqlite store (`cache.db`,
/// `cache.db-wal`, `cache.db-shm`) and the discovery-home runtime files
/// (`daemon.log`, `daemon.sock`, `daemon.pid`). Matched by basename so the
/// filter works for both a per-root `.dl/` home and the rootless XDG home.
/// `.dl/*.dl` program files (incl. `marks.dl`) are deliberately NOT matched —
/// those must still fire program/discovery reloads.
pub fn is_daemon_internal(path: &Path) -> bool {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => {
            name == "cache.db"
                || name.starts_with("cache.db-")
                || name == "daemon.log"
                || name == "daemon.sock"
                || name == "daemon.pid"
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("watchgate-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn gitignored_path_dropped_source_kept() {
        let root = tmp().join("wg1");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("target/out"), "junk").unwrap();
        let gate = WatchGate::new(&[root.clone()]);
        let kept = gate.filter(vec![
            root.join("src/main.rs"),
            root.join("target/out"),
        ]);
        assert!(kept.iter().any(|p| p.ends_with("src/main.rs")), "source kept");
        assert!(!kept.iter().any(|p| p.ends_with("target/out")), "gitignored dropped");
    }

    #[test]
    fn git_objects_dropped_head_kept() {
        let root = tmp().join("wg2");
        let _ = fs::remove_dir_all(&root);
        let gitdir = root.join(".git");
        fs::create_dir_all(gitdir.join("objects/ab")).unwrap();
        fs::create_dir_all(gitdir.join("refs/heads")).unwrap();
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(gitdir.join("objects/ab/cdef"), "blob").unwrap();
        let mut gate = WatchGate::new(&[root.clone()]);
        gate.add_git_dir(&gitdir);
        let kept = gate.filter(vec![
            gitdir.join("HEAD"),
            gitdir.join("objects/ab/cdef"),
            gitdir.join("refs/heads/main"),
        ]);
        assert!(kept.iter().any(|p| p.ends_with("HEAD")), "HEAD kept");
        assert!(kept.iter().any(|p| p.ends_with("refs/heads/main")), "refs kept");
        assert!(!kept.iter().any(|p| p.ends_with("objects/ab/cdef")), "objects churn dropped");
        assert!(gate.touches_git(&kept), "narrow ref path routes to git");
    }

    #[test]
    fn duplicates_collapse_and_daemon_internal_dropped() {
        let root = tmp().join("wg3");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".dl")).unwrap();
        fs::write(root.join("a.rs"), "x").unwrap();
        let gate = WatchGate::new(&[root.clone()]);
        let kept = gate.filter(vec![
            root.join("a.rs"),
            root.join("a.rs"),
            root.join(".dl/cache.db"),
            root.join(".dl/daemon.sock"),
        ]);
        assert_eq!(kept.iter().filter(|p| p.ends_with("a.rs")).count(), 1, "dedup");
        assert!(!kept.iter().any(|p| p.ends_with("cache.db")), "cache.db internal");
        assert!(!kept.iter().any(|p| p.ends_with("daemon.sock")), "sock internal");
    }

    #[test]
    fn daemon_internal_matches_bookkeeping_not_programs() {
        let dl = Path::new("/repo/.dl");
        for name in ["cache.db", "cache.db-wal", "cache.db-shm", "daemon.log",
                     "daemon.sock", "daemon.pid"] {
            assert!(is_daemon_internal(&dl.join(name)), "{name} should be internal");
        }
        // Program files (incl. marks) must still fire: NOT internal.
        for name in ["prog.dl", "marks.dl", "mark-lens.dl", "rails.dl"] {
            assert!(!is_daemon_internal(&dl.join(name)), "{name} must not be internal");
        }
        assert!(!is_daemon_internal(Path::new("/repo/src/main.rs")));
        // A source file that merely starts "cache.db" must NOT be internal.
        assert!(!is_daemon_internal(Path::new("/repo/src/cache.dbx.rs")));
    }

    #[test]
    fn out_of_root_retained() {
        let root = tmp().join("wg4");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let gate = WatchGate::new(&[root.clone()]);
        let other = tmp().join("elsewhere/config.toml");
        let kept = gate.filter(vec![other.clone()]);
        assert!(kept.contains(&other), "out-of-root path is not silently swallowed");
    }

    /// A submodule's contents are not gitignored (git ignores the fence for
    /// tracked gitlinks, but nothing on disk marks these files as ignored),
    /// so this is the nested-repo prune, not the gitignore matcher.
    #[test]
    fn nested_repo_dir_form_pruned_sibling_kept() {
        let root = tmp().join("wg5");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("vendor/nested/.git")).unwrap();
        fs::write(root.join("vendor/nested/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(root.join("vendor/nested/lib.rs"), "fn nested() {}").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        let gate = WatchGate::new(&[root.clone()]);
        let kept = gate.filter(vec![
            root.join("vendor/nested/lib.rs"),
            root.join("src/main.rs"),
        ]);
        assert!(!kept.iter().any(|p| p.ends_with("vendor/nested/lib.rs")),
            "nested repo content dropped: {kept:?}");
        assert!(kept.iter().any(|p| p.ends_with("src/main.rs")), "sibling source file kept");
    }

    /// A submodule worktree's `.git` is a FILE ("gitdir: <path>"), not a
    /// directory — the prune must not assume a directory form.
    #[test]
    fn nested_repo_submodule_file_form_pruned() {
        let root = tmp().join("wg6");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("vendor/sub")).unwrap();
        fs::write(root.join("vendor/sub/.git"), "gitdir: /elsewhere/modules/sub\n").unwrap();
        fs::write(root.join("vendor/sub/main.go"), "package sub").unwrap();
        let gate = WatchGate::new(&[root.clone()]);
        let kept = gate.filter(vec![root.join("vendor/sub/main.go")]);
        assert!(kept.is_empty(), "submodule worktree content must be pruned: {kept:?}");
    }

    /// The nested-repo prune must not disturb the root's OWN `.git` ref
    /// handling: `HEAD` still routes through the narrow git-dir arm.
    #[test]
    fn root_own_git_head_behavior_unchanged() {
        let root = tmp().join("wg7");
        let _ = fs::remove_dir_all(&root);
        let gitdir = root.join(".git");
        fs::create_dir_all(gitdir.join("refs/heads")).unwrap();
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let mut gate = WatchGate::new(&[root.clone()]);
        gate.add_git_dir(&gitdir);
        let kept = gate.filter(vec![gitdir.join("HEAD")]);
        assert!(kept.iter().any(|p| p.ends_with("HEAD")), "root's own HEAD still kept: {kept:?}");
        assert!(gate.touches_git(&kept), "HEAD still routes to git");
    }
}

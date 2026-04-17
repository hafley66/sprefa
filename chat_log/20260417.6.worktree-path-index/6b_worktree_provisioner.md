# 6b — WorktreeProvisioner

Lazy, demand-driven `git worktree add` per (repo, rev). Shells to native git.
Owns a cache directory and an in-memory mapping.

## Types

```rust
# v2/src/readers/_5_worktree.rs
pub struct WorktreeProvisioner {
    root:     Arc<Path>,                                   # cache dir root
    locator:  Arc<dyn CheckoutLocator>,                    # resolves repo → clone path
    mapping:  RwLock<HashMap<(Arc<str>, Arc<str>), Arc<Path>>>,
}

impl WorktreeProvisioner {
    pub fn new(root: Arc<Path>, locator: Arc<dyn CheckoutLocator>) -> Self;

    pub fn ensure(&self, repo: &str, rev: &str) -> Option<Arc<Path>>;
    # fast-path: mapping hit → return Arc<Path>
    # miss: acquire per-(repo,rev) lock, re-check, shell add, insert, return

    pub fn drop_rev(&self, repo: &str, rev: &str);
    # remove from mapping; shell `git worktree remove --force <wt>`

    pub fn list(&self) -> Vec<(Arc<str>, Arc<str>, Arc<Path>)>;
    # for observability; not in hot path
}
```

## Ensure body shape

```rust
fn ensure(&self, repo, rev) -> Option<Arc<Path>> {
    if let Some(p) = self.mapping.read().get(&(repo, rev)) { return Some(p.clone()) }

    let clone_path = self.locator.resolve(repo)?;
    let wt_dir = self.root.join(sanitize(repo)).join(sanitize(rev));

    # shell: git -C <clone> worktree add --detach --quiet <wt_dir> <rev>
    let status = Command::new("git")
        .args(["-C", clone_path, "worktree", "add", "--detach", "--quiet",
               wt_dir.as_os_str(), rev])
        .status().ok()?;
    if !status.success() { return None }

    let arc: Arc<Path> = Arc::from(wt_dir.as_path());
    self.mapping.write().insert((repo, rev), arc.clone());
    Some(arc)
}
```

## Concurrency

- `RwLock<HashMap>` for the mapping — read-hot, write-cold
- Double-checked locking on insert (read, then write, re-check)
- No cross-process lock — single-daemon assumption (same as SqliteStore)
- Per-(repo,rev) serialization via an entry-scoped `Mutex` inside a DashMap
  to avoid racing `git worktree add` on the same pair

## Failure modes

- rev unknown → `status != 0` → return None → caller falls back to libgit2
- disk full → same
- wt dir already exists (stale) → attempt `worktree repair` or remove + retry
  once; on second failure, return None

## Blast radius

- `v2/src/readers/_5_worktree.rs` — new file, ~150 lines
- `v2/src/readers/mod.rs` — register module
- Tests: one local fixture with two revs, `ensure` idempotent, `drop_rev`
  cleans disk + mapping

## Depends on / depended on by

- Depends: `CheckoutLocator` (already exists, powers rev → clone path today)
- Depended on: 6d (reader branches), 6e (config owns the cache root)

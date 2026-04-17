# 6d — GitBlobReader 3-phase files()

Replace the single libgit2 walk with: index hit → WT materialize + OS walk →
libgit2 fallback.

## Signature unchanged

```rust
# v2/src/readers/_2_git.rs
impl Reader for GitBlobReader {
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern)
        -> BoxStream<'static, Vec<FilePath>>;
}
```

## New fields

```rust
pub struct GitBlobReader {
    # existing: repos, trees
    path_index:   Arc<dyn PathIndex>,
    provisioner:  Option<Arc<WorktreeProvisioner>>,    # None = disabled
}
```

## Body shape

```rust
fn files(&self, repo, rev, pattern) -> BoxStream<Vec<FilePath>> {
    # Phase A — index hit
    if let Some(hit) = self.path_index.files_at(repo, rev, pattern) {
        return once_val(hit);
    }

    # Phase B — WT materialize + OS walk + index write
    if let Some(wt) = self.provisioner.as_ref().and_then(|p| p.ensure(repo, rev)) {
        let all = walk_wt_with_ls_tree(&wt, repo, rev);      # Vec<(FilePath, oid)>
        self.path_index.upsert_rev(repo, rev, &all);
        let matched = all.into_iter()
            .map(|(fp, _)| fp)
            .filter(|fp| pattern.is_match(fp.as_str()))
            .collect();
        return once_val(matched);
    }

    # Phase C — libgit2 fallback (today's path)
    self.walk_tree_libgit2(repo, rev, pattern)
}
```

## walk_wt_with_ls_tree

```rust
fn walk_wt_with_ls_tree(wt: &Path, repo: &str, rev: &str)
    -> Vec<(FilePath, [u8; 20])>
{
    # `git -C <wt> ls-tree -r <rev>` gives (mode, type, oid, path) per blob
    # parse lines, filter to blobs, build (FilePath, oid) tuples
    # ~10-50ms on swc — native git is the fastest tree walker
}
```

Why `ls-tree` over `ignore::WalkBuilder`:
- gets oids alongside paths — reuses future `blob_oid()` fast path
- matches git's own byte view; no `.gitignore` mismatch risk
- WalkBuilder is fine for the bytes path; enumeration wants oids

## Fallback policy

| phase hit | cold wall | warm wall |
|---|---|---|
| A — index | n/a | <5ms |
| B — WT + ls-tree + index | ~3s (git) + insert | n/a |
| C — libgit2 | 14s | 14s |

Phase C lives for: misconfigured cache dir, `git worktree add` failure
(corrupt clone, permissions), `path_index` disabled.

## Blast radius

- `v2/src/readers/_2_git.rs` — add fields, constructor change, rewrite `files()`
- `v2/src/readers/_2_git.rs` — helper `walk_wt_with_ls_tree`
- `v2/src/server/_1_workspace.rs` — pass `path_index` + `provisioner` to ctor
- Tests: each of A/B/C hit separately; WT hit writes index; index hit skips WT

## Depends on / depended on by

- Depends: 6b (WorktreeProvisioner), 6c (PathIndex)
- Depended on: 6e (config surface), 6g (bench)

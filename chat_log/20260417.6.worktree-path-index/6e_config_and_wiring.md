# 6e — Config surface + workspace wiring (Z3)

Flip the switch: wire `Option<Arc<WorktreeProvisioner>>` and
`Option<Arc<dyn PathIndex>>` into every `GitBlobReader` construction site
so phases 1 and 2 fire in production.

## Ground truth from landed 6b/6c/6d

- `GitBlobReader::new(locator, config)` still exists, delegates to
  `new_with_index(..., None, None)`. Do NOT remove the old ctor.
- `GitBlobReader::new_with_index(locator, config, provisioner, path_index)`
  is the wiring target.
- `SqlxPathIndex::open(&Path) -> Result<Arc<Self>, PathIndexErr>`
- `WorktreeProvisioner::new(cache_root: PathBuf) -> Self`
- Eight `GitBlobReader::new` call sites exist (per 6d audit):
  - 2 in `v2/src/server/_1_workspace.rs`
  - 2 in `v2/src/bin/sprefa_v2_lsp.rs`
  - 1 in `v2/src/readers/git_read.rs`  (check actual path — may be test helper)
  - 3 in `v2/src/ops/.../rule_json_git.rs` (probably ops test support)

Only the production sites (server + lsp) need to switch to `new_with_index`.
Test/helper sites stay on `new` — the `None/None` default is correct there.

## Step 1 — RuntimeConfig additions

```rust
# v2/src/_2_config.rs
use std::path::PathBuf;

pub struct RuntimeConfig {
    # ... existing fields ...
    /// Directory for git-worktree cache. None = feature off.
    pub worktree_cache_dir: Option<PathBuf>,
    /// Path for the SqlxPathIndex SQLite DB. None = feature off.
    pub path_index_db:      Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn test_default() -> Self {
        Self {
            # ... existing fields ...
            worktree_cache_dir: None,
            path_index_db:      None,
        }
    }
}
```

No enum. Two `Option<PathBuf>`. Both `None` → current behavior, sync fast-path
in `GitBlobReader::files` kicks in. Both `Some(_)` → full phases land.
Partial wiring (one Some one None) is legal — reader treats missing
provisioner as "phase 2 skipped, fall through to phase 3."

## Step 2 — .sprefa.toml surface

```toml
[runtime]
worktree_cache_dir = "~/.cache/sprefa/wt"    # optional; unset = off
path_index_db      = "~/.cache/sprefa/path_index.sqlite"  # optional; unset = off
```

Parse via the existing TOML path. Tilde expansion: use `shellexpand::tilde`
only if already in deps — otherwise accept absolute paths only and document.

## Step 3 — Wire DocSession / Workspace ctor

The actual construction site is `v2/src/server/_1_workspace.rs`. Read the
file to see exactly which function holds the `GitBlobReader::new` calls;
likely `build_workspace_ctx` or `DocSession::new`.

Sketch:

```rust
# v2/src/server/_1_workspace.rs
async fn build_workspace_ctx(root: &Path, cfg: Arc<Config>) -> Result<Arc<WorkspaceCtx>, ...> {
    # ... existing config resolution, locator construction ...

    # --- path index (optional) ---
    let path_index: Option<Arc<dyn PathIndex>> =
        match cfg.runtime.path_index_db.as_ref() {
            Some(db_path) => {
                let idx = SqlxPathIndex::open(db_path).await
                    .map_err(|e| anyhow!("path_index open {}: {e}", db_path.display()))?;
                Some(idx as Arc<dyn PathIndex>)
            }
            None => None,
        };

    # --- worktree provisioner (optional) ---
    let provisioner: Option<Arc<WorktreeProvisioner>> =
        cfg.runtime.worktree_cache_dir.as_ref().map(|dir| {
            Arc::new(WorktreeProvisioner::new(dir.clone()))
        });

    # --- swap ctor call ---
    let git = Arc::new(GitBlobReader::new_with_index(
        locator.clone(),
        cfg.clone(),
        provisioner.clone(),
        path_index.clone(),
    ));

    # wrap in ParseCacheReader → BufferOverlay as today
    let reader = Arc::new(BufferOverlay::new(Arc::new(
        ParseCacheReader::new(git, /* parse cache args */)
    )));

    # ... existing WorkspaceCtx assembly ...
}
```

Do the same swap in the LSP entry point `v2/src/bin/sprefa_v2_lsp.rs` (both
`GitBlobReader::new` sites).

Test/helper sites (`rule_json_git.rs`, `git_read.rs` if it's a helper): leave
on `GitBlobReader::new`. They don't need the index; default `None` is
correct.

## Step 4 — OpCtx unchanged

Per v2/CLAUDE.md invariant #1: ops stay oblivious. `path_index` lives on the
reader, not on `OpCtx`. `OpCtx::for_test` does NOT gain a field. If a test
specifically wants to exercise the index, it constructs a reader with
`new_with_index` and feeds that into `OpCtx::for_test` via the existing
reader slot.

## Step 5 — Config parse test

```rust
# v2/src/_2_config.rs inline tests
#[test]
fn runtime_parses_worktree_and_path_index() {
    let toml = r#"
        [runtime]
        worktree_cache_dir = "/tmp/sprefa/wt"
        path_index_db      = "/tmp/sprefa/pi.sqlite"
    "#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.runtime.worktree_cache_dir.as_deref(),
               Some(std::path::Path::new("/tmp/sprefa/wt")));
    assert_eq!(cfg.runtime.path_index_db.as_deref(),
               Some(std::path::Path::new("/tmp/sprefa/pi.sqlite")));
}

#[test]
fn runtime_defaults_off_when_fields_absent() {
    let toml = "[runtime]\n";
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(cfg.runtime.worktree_cache_dir.is_none());
    assert!(cfg.runtime.path_index_db.is_none());
}
```

## Step 6 — Workspace ctor smoke test

```rust
# v2/src/server/_1_workspace.rs inline tests
#[tokio::test]
async fn workspace_with_index_wires_reader() {
    let tmp = tempfile::TempDir::new().unwrap();
    let cfg = Config {
        runtime: RuntimeConfig {
            worktree_cache_dir: Some(tmp.path().join("wt")),
            path_index_db:      Some(tmp.path().join("pi.sqlite")),
            ..RuntimeConfig::test_default()
        },
        ..Config::test_default()
    };
    let ctx = build_workspace_ctx(tmp.path(), Arc::new(cfg)).await.unwrap();
    # verify the reader actually holds Some provisioner + Some path_index —
    # either via a downcast helper or by observing phase-1-hit behavior.
}
```

## Absolute stop conditions

- Adding fields to `OpCtx`.
- Touching more than: `_2_config.rs`, `server/_1_workspace.rs`,
  `bin/sprefa_v2_lsp.rs`, plus test updates in same files.
- Swapping `new` → `new_with_index` in test/helper files.
- Adding new dependencies (no `shellexpand` unless already present).
- Modifying `PathIndex` / `WorktreeProvisioner` / `GitBlobReader` APIs
  (they're frozen after 6b/6c/6d).

## Blast radius

| file | change | lines |
|---|---|---|
| `v2/src/_2_config.rs` | 2 new fields + `test_default` + 2 tests | +30 |
| `v2/src/server/_1_workspace.rs` | Option→ctor wiring + smoke test | +40 |
| `v2/src/bin/sprefa_v2_lsp.rs` | 2 ctor swaps | +20 |

## Verify

```
cd v2 && cargo build --tests 2>&1 | tail -20
cd v2 && cargo test -p v2 --lib config 2>&1 | tail -10
cd v2 && cargo test -p v2 --lib workspace 2>&1 | tail -10
cd v2 && cargo test -p v2 --lib 2>&1 | tail -5    # must stay ≥250 passed
```

## Depends on / depended on by

- Depends: 6b, 6c, 6d (all landed).
- Depended on: 6g (needs the production wiring to measure cold/warm).

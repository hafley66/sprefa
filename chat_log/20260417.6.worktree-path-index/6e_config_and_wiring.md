# 6e — Config surface + workspace wiring

Expose knobs in `RuntimeConfig`, thread provisioner + path_index through
`build_workspace_ctx`.

## RuntimeConfig

```rust
# v2/src/_2_config.rs
pub struct RuntimeConfig {
    # existing: max_passes, max_claims_per_pass, max_cursors_per_root,
    #           buffer_size, parse_cache_dir, ...
    pub worktree_cache_dir: Option<PathBuf>,
    pub worktree_max_revs:  usize,            # LRU cap; 0 = unbounded
    pub path_index_mode:    PathIndexMode,
}

pub enum PathIndexMode {
    Off,              # no index, no WT; phase C only (today)
    Memory,           # in-process NoopStore variant with HashMap? or skip
    Sqlite,           # production: SqliteStore path index
}

impl RuntimeConfig {
    pub fn test_default() -> Self {
        Self {
            # ...
            worktree_cache_dir: None,
            worktree_max_revs:  0,
            path_index_mode:    PathIndexMode::Off,
        }
    }
}
```

Default for production server: `Sqlite` + `$XDG_CACHE_HOME/sprefa/wt`.
Default for tests: `Off` (today's behavior preserved).

## .sprefa.toml surface

```toml
[runtime]
worktree_cache_dir = "~/.cache/sprefa/wt"      # or unset → XDG default
worktree_max_revs  = 50
path_index_mode    = "sqlite"                  # or "off"
```

## Workspace ctor

```rust
# v2/src/server/_1_workspace.rs
fn build_workspace_ctx(root: &Path) -> Arc<WorkspaceCtx> {
    # ... existing config resolution ...

    let path_index: Arc<dyn PathIndex> = match cfg.runtime.path_index_mode {
        PathIndexMode::Off    => Arc::new(NoopStore::new()) as _,
        PathIndexMode::Sqlite => Arc::new(SqliteStore::open(&cfg)?) as _,
    };

    let provisioner = cfg.runtime.worktree_cache_dir.as_ref().map(|dir| {
        Arc::new(WorktreeProvisioner::new(
            Arc::from(dir.as_path()),
            locator.clone(),
        ))
    });

    let git = Arc::new(GitBlobReader::new_with(
        locator.clone(),
        path_index.clone(),
        provisioner.clone(),
    ));

    # wrap in ParseCacheReader → BufferOverlay as today
    # ...
}
```

## OpCtx

No change. `path_index` lives on the reader, not on `OpCtx`. Ops stay
unaware; this is reader-internal optimization.

## Blast radius

- `v2/src/_2_config.rs` — new fields + enum, `test_default` updated
- `v2/src/server/_1_workspace.rs` — ctor threading
- `v2/src/_5_op.rs::OpCtx::for_test` — no change (reader ctor handles defaults)
- Tests: config parse round-trip, ctor picks right impl per mode

## Test scaffold discipline

New `RuntimeConfig` fields MUST have defaults in `test_default()`. No test
files touched. If compile breaks a test, fix is in `test_default`.

## Depends on / depended on by

- Depends: 6b, 6c, 6d
- Depended on: 6g

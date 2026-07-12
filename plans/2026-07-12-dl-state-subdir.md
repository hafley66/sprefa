# Move engine runtime files into `.dl/.state/`

## Why
A repo's `.dl/` mixes the user's committed `.dl` programs (green in the editor)
with the engine's gitignored runtime blobs (`cache.db*`, `index.scip*`). Nest the
runtime blobs one level down in `.dl/.state/` so `.dl/` shows only authored files.

Scope: the two engine-written artifacts only.
- `cache.db`, `cache.db-shm`, `cache.db-wal` — the per-repo one-shot store
  (`src/cli/mod.rs:315`, the `db_defaulted` path).
- `index.scip*` — the SCIP index (`src/scip_import.rs`, `src/scip_setup.rs`).

OUT of scope (do not touch):
- `daemon_home()` (`$XDG_STATE_HOME/sprefa`) and its `daemon.sock/pid/log`,
  `roots/<hash>/db.sqlite`. That is not a repo `.dl/`; leave it. The stale
  `daemon.*` files sitting in some repos' `.dl/` are legacy from an older home
  model; current code does not recreate them there.

## Target layout
    <root>/.dl/.state/cache.db{,-shm,-wal}
    <root>/.dl/.state/index.scip
    <root>/.dl/.gitignore   ->   contains `.state/`

## Central helper (new)
Add ONE place that names the dir and creates it. Put in `src/lib.rs` (or a tiny
`src/statedir.rs` module, exported):

    /// `<root>/.dl/.state` — engine runtime blobs, gitignored, out of the
    /// authored-`.dl` view. Created on demand.
    pub fn state_dir(root: &Path) -> PathBuf { root.join(".dl").join(".state") }

## Sites
1. `src/cli/mod.rs` ~305-317 (default-db block):
   - `let dl = root.join(".dl"); let state = state_dir(&root);`
   - ensure `.gitignore` in `dl` contains a `.state/` line (append if the file
     exists but lacks it; create with `.state/\n` if absent). Keep any existing
     lines — do not clobber a user's `.gitignore`.
   - `std::fs::create_dir_all(&state)` before use.
   - **Migration**: if `dl/cache.db` exists and `state/cache.db` does not, rename
     `cache.db`, `cache.db-shm`, `cache.db-wal` into `state/` (best-effort; a
     missing -shm/-wal is fine). Do it before setting `db`.
   - `db = Some(state.join("cache.db")...)`.
   - Gate `dir.is_dir()` stays on the `.dl` dir existing (unchanged trigger).
2. `src/scip_import.rs::index_path` (~67): add `root/.dl/.state/index.scip` as a
   candidate. Order: `$SPREFA_SCIP_INDEX`, `<root>/index.scip`,
   `<root>/.dl/.state/index.scip`, `<root>/.dl/index.scip` (keep the old path last
   so pre-move indexes still resolve).
3. `src/scip_setup.rs`:
   - `index_out`/wherever it writes (`.dl/index.scip`, ~182): write to
     `.dl/.state/index.scip`; `create_dir_all` the `.state` dir first.
   - `gitignore_index` (~187): ensure `.dl/.gitignore` contains `.state/`
     (replaces the `index.scip*` entry logic — a single `.state/` line covers it).
     Keep idempotent + preserve existing entries (its two tests assert this).
   - Update the eprintln hints that print `<root>/.dl/index.scip`.
4. `src/watchgate.rs::is_daemon_internal` (~214): basename match already covers
   `cache.db*` at any depth, so files under `.state/` stay internal. Confirm the
   walker still treats a `.state/cache.db` write as internal (add a test row for
   `.dl/.state/cache.db`). `index.scip` dirties via ScipKind separately — verify
   the `.state/index.scip` path still maps to ScipKind (grep watchgate/scip for
   the `index.scip` basename match and update if it pins the `.dl/` depth).
5. `src/hook.rs:305`, `src/lib.rs:331` eprintln strings mention
   `.dl/cache.db` — update to `.dl/.state/cache.db`.

## Tests (RED->GREEN, count-based where possible)
- New unit/e2e in `tests/it/` (or extend an existing setup test): a default one-shot
  run creates `.dl/.state/cache.db`, NOT `.dl/cache.db`; `.dl/.gitignore` contains
  `.state/`.
- Migration test: pre-place a `.dl/cache.db`, run, assert it moved to
  `.dl/.state/cache.db` and the old path is gone.
- `src/scip_setup.rs` tests (`gitignore_added_once_and_idempotent`,
  `gitignore_preserves_existing_entries`, and the `index.scip*` assertions):
  update to the `.state/` entry; keep idempotency + preserve-existing coverage.
- `src/scip_import.rs` index_path test: add a `.dl/.state/index.scip` resolves case.
- `src/watchgate.rs` tests: add `.dl/.state/cache.db` is internal.

## Hermetic runs
`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, scratch `--db` where a test
needs a store that is NOT the default path (the default-path tests must let the
engine pick `.dl/.state/cache.db`, so run with cwd = a temp repo and no `--db`).

## Laws
- N+1: no per-row writes introduced (none here).
- No `provenance`/`substrate`/`load-bearing`/`regime` identifiers.
- Descriptive dl/rust var names, no single letters.
- File-size: touched files stay under their current size class; no big new module.
- Commit protocol: one commit for the move+migration+helper, one for scip, one for
  tests is fine — or a single cohesive commit. `git commit -n`. Do NOT push.

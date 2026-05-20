# Plan: daemon-owned FS watcher + VFS overlay + reactive wake

## Goal

Two sources of file-change events (OS watcher + LSP `did_change`) funnel into ONE wake ingress in the daemon. Wake emits `RuntimeJob`s for owners that read the changed file (via `MEMO_DEPS`). Expand drains jobs with `warm_slice=[changed_path]` and overlay-aware reads. Result: typing in the IDE → ≤100ms diagnostics; saving + outside-IDE edits also wake the same path.

## Architecture (one diagram, then signatures)

```
   FsWatcher (notify::Watcher, debouncer)
        │ FsEvent { path, kind }
        ▼
                                                  did_change (uri, text, version)
   ┌───────────────────────────┐                             │
   │ EventIngress::on(event)   │ ◀──────── RPC ──────────────┤
   │   → Vec<RuntimeJob>       │                             │
   └─────────┬─────────────────┘                  ┌──────────▼─────────┐
             │                                    │ VfsOverlay         │
             │                                    │ Map<PathBuf, Text> │
             │                                    └────────────────────┘
             ▼
   pending_jobs mpsc (bounded)
             │
             ▼
   expand drain (50ms tick OR get_diags pull)
   warm_slice=[changed], reads consult VfsOverlay first
             │
             ▼
   publish_diagnostics(uri) → IDE
```

## Type signatures

### Daemon-side state (extends `AppState`)

```rust
pub struct AppState {
    // ... existing fields ...

    /// One watcher per daemon instance. Watches only the dirs that
    /// contain a file appearing in `_memo_deps`. Refresh set whenever
    /// MEMO_DEPS grows.
    fs_watcher: Arc<Mutex<FsWatcherState>>,

    /// Buffer overlay. IDE-owned text takes precedence over disk
    /// when `SourceReader` resolves a path.
    vfs: Arc<VfsOverlay>,

    /// Single mpsc for ingress → expand handoff. Bounded so a burst
    /// can't OOM the daemon; backpressure on the FsWatcher channel
    /// when the drain is behind.
    pending_jobs: mpsc::Sender<RuntimeJob>,
}

struct FsWatcherState {
    watcher: notify::RecommendedWatcher,
    watched_dirs: HashSet<PathBuf>,
    /// Reverse index for O(1) ingress lookup. Derived from
    /// MEMO_DEPS distinct paths.
    paths_of_interest: HashSet<PathBuf>,
}

pub struct VfsOverlay {
    /// Absolute-path → in-memory text. None means "owned by IDE,
    /// unsaved"; lookups fall back to disk for paths absent here.
    buffers: DashMap<PathBuf, Arc<str>>,
}

impl VfsOverlay {
    pub fn put(&self, path: PathBuf, text: Arc<str>);
    pub fn remove(&self, path: &Path) -> Option<Arc<str>>;
    pub fn get(&self, path: &Path) -> Option<Arc<str>>;
    pub fn is_owned(&self, path: &Path) -> bool;
}
```

### Event types

```rust
pub enum WakeEvent {
    /// On-disk change observed by FsWatcher (post-debounce).
    /// Always followed by an identity-equality check before clock bump.
    FsChange { path: PathBuf, kind: notify::EventKind },

    /// LSP buffer change. `text` overlays disk; `path` resolved from
    /// `uri` by the LSP frontend before the RPC. `version` is the
    /// LSP doc version; ingress drops stale wakes by version.
    BufferChange { path: PathBuf, text: Arc<str>, version: i32 },

    /// LSP buffer close → drop overlay, resume disk reads, treat as
    /// FsChange so any external edits since open are noticed.
    BufferClose { path: PathBuf },
}
```

### EventIngress (the single funnel)

```rust
impl AppState {
    /// Project ONE event to wake jobs. Idempotent; safe to call
    /// concurrently from FsWatcher task and LSP RPC handler.
    pub fn on_wake_event(&self, e: WakeEvent) -> Vec<RuntimeJob> {
        // Pseudo-code:
        // 1. resolve path → SourceId via SourceId::for_file(path)
        // 2. classify:
        //    FsChange:
        //      if vfs.is_owned(path) { return vec![] }   // buffer suppresses
        //      let new_id = source_index.identity(path);
        //      let recorded = recorded_dep_identities()[path];
        //      if new_id == recorded { return vec![] }   // chmod / touch
        //      durable_clock.bump(sid);
        //    BufferChange { text, version }:
        //      vfs.put(path, text);
        //      ephemeral_clock.bump(sid);
        //    BufferClose:
        //      vfs.remove(path);
        //      durable_clock.bump(sid);  // disk truth resumes
        //
        // 3. owners = memo_deps_by_source.get(&sid.0)   // O(1)
        // 4. owners.map(|o| RuntimeJob {
        //      owner: o,
        //      warm_slice: vec![path.clone()],
        //      reason: WakeReason::SourceChanged(sid),
        //    }).collect()
    }
}
```

### MEMO_DEPS reverse index (extends `memo_deps_cache`)

```rust
// runtime_graph.rs:118-128 today holds the forward cache.
// Add the reverse:
pub struct MemoDepsCache {
    forward:  HashMap<(OwnerKey, InKey), Vec<DepRow>>,
    by_source: HashMap<[u8; 32], HashSet<(OwnerKey, InKey)>>, // NEW
}

impl MemoDepsCache {
    pub fn owners_of(&self, sid: SourceId) -> impl Iterator<Item = &(OwnerKey, InKey)>;
}
```

### SourceReader overlay seam

```rust
// v4/src/source.rs:40 today goes straight to disk.
impl SourceReader {
    fn read_cursor_with_intern(&self, c: &Cursor, intern: bool) -> Option<SourceBytes> {
        let abs = self.resolve_cursor_path(c)?;
        // NEW: overlay precedence.
        if let Some(text) = self.vfs.get(&abs) {
            return Some(SourceBytes::from_overlay(abs, text));
        }
        // existing disk-read path
    }
}
```

### Two clocks (replaces today's single `SourceClock`)

```rust
pub struct Clocks {
    /// Durable: bumped on FsChange (disk truth moved) AND on
    /// BufferClose. Memo replay checks THIS clock.
    pub durable: Arc<FactStoreClock>,
    /// Ephemeral: bumped on BufferChange. Lives in-RAM only; lost
    /// at process restart. Live-LSP eval checks THIS clock.
    pub ephemeral: Arc<InMemClock>,
}

impl Clocks {
    pub fn current_gen(&self, sid: SourceId) -> u64 {
        // max of durable + ephemeral; ensures any movement = wake.
        std::cmp::max(self.durable.current_gen(sid), self.ephemeral.current_gen(sid))
    }
}
```

## Instance lifetimes (who owns what, who outlives whom)

- `AppState`: process-lifetime singleton. Built once at daemon start.
- `FsWatcherState`: lives inside `AppState`. Built on first MEMO_DEPS load OR on first `lsp_open`, whichever comes first. Watcher task lives in a tokio task spawned at construction; dies with the daemon.
- `VfsOverlay`: lives inside `AppState`. Entries created by `lsp_open` / `BufferChange`, removed by `BufferClose`. Buffer entries are `Arc<str>`; multiple readers share without clone.
- `pending_jobs` mpsc: lives inside `AppState`. Receiver owned by the drain task; sender cloned to FsWatcher task + RPC handler.
- `MemoDepsCache::by_source`: rebuilt on every `record_memo_deps` write-through. Cost: one HashSet::insert per dep recorded.
- `Clocks::ephemeral`: process-lifetime singleton. Reset on restart. NOT persisted.

## Storage layout (where each piece writes)

| state | medium | sized by |
|---|---|---|
| `vfs.buffers` | DashMap in-RAM | # open LSP buffers (~10–50) |
| `fs_watcher.paths_of_interest` | HashSet in-RAM | # distinct paths in MEMO_DEPS |
| `pending_jobs` | bounded mpsc, cap 1024 | burst size |
| `memo_deps_by_source` | HashMap in-RAM | # distinct SourceIds in MEMO_DEPS |
| `Clocks::ephemeral` | atomic counters keyed by SourceId | # files ever buffered |
| `_memo_deps` table | sqlite, existing | # (owner, in_key, source) triples |

## Sequence of reads and writes (the hot path)

### Cold start
1. Daemon starts. `_memo_deps` is read into RAM (existing).
2. `by_source` reverse index built from that read.
3. `paths_of_interest` derived from `by_source` (distinct paths via `SELECT DISTINCT source_path FROM _memo_deps`).
4. `FsWatcher` registered for the containing dir of every path in `paths_of_interest`. Recursive=false (one dir at a time).
5. drain task spawned, awaits `pending_jobs.recv()`.

### IDE keystroke
1. IDE → sprefa-lsp: `did_change(uri, text, version)`.
2. sprefa-lsp resolves uri→path, sends `WakeEvent::BufferChange` over RPC to daemon.
3. Daemon's RPC handler calls `on_wake_event(BufferChange)`:
   a. `vfs.put(path, text)`.
   b. `ephemeral_clock.bump(sid)`.
   c. `owners = by_source.get(&sid.0)`.
   d. Push RuntimeJobs into mpsc.
4. Drain task wakes (mpsc recv). Batches via `try_recv` until empty.
5. Drain task calls `expand` with `warm_slice=[path]` and the new ctx (overlay-aware reader).
6. `expand` runs ONLY the affected rules. `fs` emits one cursor (the changed path). `ast` reads via `vfs.get(path)` → buffer text, no disk hit.
7. Diags collected, published to LSP frontend, frontend → IDE.

### Outside-IDE edit (e.g., git pull)
1. notify fires on the dir → debouncer collapses ~100ms of events into one.
2. FsWatcher task sends `WakeEvent::FsChange { path }` → mpsc.
3. Drain task picks up. `on_wake_event(FsChange)`:
   a. `vfs.is_owned(path)` → false.
   b. New `SourceIdentity::for(path)` vs recorded → moved → bump `durable_clock`.
   c. `owners = by_source.get(&sid.0)`.
   d. Push RuntimeJobs.
4. Expand runs (same as IDE path; reads disk because vfs has no overlay for this path).
5. Diags published. If any IDE client has the file open as a related dep, they see the wake.

### IDE close
1. `did_close(uri)` → LSP frontend → daemon `WakeEvent::BufferClose`.
2. `vfs.remove(path)` → disk-truth resumes.
3. Treat as FsChange to catch any external edits since the buffer was open.

## Uniqueness conditions

- **One watcher per workspace.** Daemon enforces via `Mutex<FsWatcherState>`. A second LSP frontend connecting to the same daemon shares the watcher; no double-watch.
- **One overlay entry per path.** `DashMap::insert` overwrites. Concurrent `BufferChange` events on the same path are linearized by the DashMap shard mutex.
- **One in-flight job per (owner, source).** Drain task dedupes the mpsc-drained batch on `(owner_op_id, in_key)` before calling expand. A burst of 20 saves on the same file = 1 expand call, not 20.
- **Clock generations are monotonic per SourceId.** Atomic fetch_add. Ephemeral and durable can interleave but neither moves backward.
- **`by_source` consistency.** Updates happen ONLY through `record_memo_deps` → both `forward` and `by_source` write-through together under the same lock. No torn state visible to ingress.

## Risks / open questions

1. **Watcher overload.** A `cargo build` triggers thousands of events under `target/`. `paths_of_interest` keeps the watch set small, but the WATCHED DIRS may be repo roots if MEMO_DEPS has dispersed entries. Mitigate: detect bursts (>100 events/sec per dir) and bypass via SourceIndex identity-equality gate, not full re-eval.

2. **Atomic-rename editors** (vim, gofmt). notify sees `Remove + Create`. `notify-debouncer-mini` collapses to `Modify`. Already standard.

3. **Symlinks / hardlinks.** SourceIdentity using git OID is hash-stable across hardlinks. Stat-tuple identity (off-git) can false-wake on hardlink swap; acceptable.

4. **Pre-warm policy.** On cold daemon start with NO MEMO_DEPS, the watcher has nothing to watch. First lsp_open kicks the first cold compile, populates MEMO_DEPS, then the watcher registers. There's a ~one-keystroke window where outside-IDE edits aren't seen until the first IDE event. Acceptable for v1.

5. **VfsOverlay leaking through `read_cursor_uninterned` for non-LSP CLI runs.** When the same daemon serves both LSP and CLI clients, the CLI shouldn't see IDE buffer text. Two paths:
   - (a) VfsOverlay is per-`RpcSession`, not global.
   - (b) Global VfsOverlay but `SourceReader::with_overlay(None)` opt-out for non-LSP entry points.
   Default to (b) for simplicity; flip to (a) if multi-tenant becomes a real concern.

6. **Buffer text version vs durable clock.** A `BufferChange v=5` may race a `BufferChange v=6` arriving from a different RPC connection. Drop events with `version < latest seen for uri`. sprefa-lsp already has the per-URI version checkpoint (`main.rs:187-189`); the daemon mirrors it.

## TODO checklist

- [ ] Add `notify` + `notify-debouncer-mini` to v4 Cargo.toml.
- [ ] Implement `VfsOverlay` (DashMap-backed) in `v4/src/app.rs` near `DocState`.
- [ ] Thread `Arc<VfsOverlay>` into `SourceReader::new`; consult before disk in `read_cursor_with_intern`.
- [ ] Build `MemoDepsCache::by_source` reverse index; write-through in `record_memo_deps`.
- [ ] `WakeEvent` enum + `on_wake_event` ingress in `AppState`.
- [ ] FsWatcher task: watcher on dirs derived from `paths_of_interest`; debouncer; sends `WakeEvent::FsChange` into mpsc.
- [ ] LSP frontend (`sprefa-lsp/src/main.rs`): on `did_change`, resolve uri → abs path, RPC `WakeEvent::BufferChange` to daemon. Existing 80ms debounce stays.
- [ ] Drain task: mpsc recv → dedupe on (owner, in_key) → call expand with `warm_slice=[paths]` and overlay-aware reader.
- [ ] Two clocks: split today's `FactStoreClock` into `durable` (existing) + `ephemeral` (in-RAM atomic counter per SourceId). `Clocks::current_gen` returns max.
- [ ] Refresh `paths_of_interest` whenever `record_memo_deps` adds a new distinct source path; re-register watcher if a new dir appears in the set.
- [ ] Buffer-owned suppression: `vfs.is_owned(path)` short-circuit in `on_wake_event(FsChange)`.
- [ ] Test: `tests/lsp_reactive_wake_target.rs`. Spawn daemon, register a 2-file lint, emit `BufferChange` for file A, assert diags publish in <100ms and only file A is parsed.
- [ ] Test: same setup, emit OS `Modify` for file B (via tempdir write), assert wake fires.
- [ ] Test: open buffer A, emit OS `Modify` for A, assert IGNORED (buffer-owned).
- [ ] Test: `BufferClose` for A after on-disk edit, assert re-read picks up disk content.
- [ ] Bench gate: 60k-file repo, 100 keystrokes/s on one buffer; assert p99 < 100ms per keystroke.

## Critical files

- `v4/src/app.rs` (AppState extension, EventIngress, drain task)
- `v4/src/source.rs` (SourceReader overlay seam)
- `v4/src/runtime_graph.rs` (MemoDepsCache by_source, two clocks)
- `v4/crates/sprefa-lsp/src/main.rs` (did_change → BufferChange RPC; did_close → BufferClose)
- `v4/src/source_clock.rs` (split durable / ephemeral)
- `v4/Cargo.toml` (notify + notify-debouncer-mini)

## Companion plans

- `v4/plans/fs-streaming-peak-memory.md` — bounded-memory FS streaming (orthogonal; can land before or after).
- `v4/plans/lsp-loop-justification-lint.md` — the rust-loop lint that USES this watcher.
- `v4/plans/lsp-sprf-component-n-plus-1-lint.md` — the sprf-component lint that USES this watcher.

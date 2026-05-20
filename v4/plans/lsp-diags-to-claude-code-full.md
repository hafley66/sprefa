# Plan: surface sprefa-lsp diagnostics in Claude Code via `mcp__ide__getDiagnostics`

## 0. Status

| piece | exists? | where |
|---|---|---|
| LSP server with `publish_diagnostics` | yes | `v4/crates/sprefa-lsp/src/main.rs:184`, `:416` |
| `did_change` 80ms debounce + version checkpoint | yes | `v4/crates/sprefa-lsp/src/main.rs:26`, `:131-189`, `:401-409` |
| `Mutex<Arc<dyn SprfClient>>` backend pattern | yes | `v4/crates/sprefa-lsp/src/main.rs:58`, `:84-92` |
| `AppState.docs: Mutex<HashMap<String, DocState>>` | yes | `v4/src/app.rs:492-501`, `:503-522` |
| RPCs `lsp_open` / `lsp_change` / `get_diags` | yes | `v4/src/app.rs:470-485`, `:1453-1476` |
| `SprfDiag` DTO + `From<&Diag>` | yes | `v4/src/app.rs:131-156` |
| `LspBodyComponent`, `lsp_error(:code)` surface | yes | `v4/src/lsp.rs:153-250`, `:284-289` |
| `AstYamlComponent` (the typical lint engine) | yes | `v4/src/v2_ops.rs:1106-1145`, `:1147-1212` |
| `warm_slice` / `warm_changed_paths` / `warm_skip_predicate` | yes | `v4/src/runtime_graph.rs:610-661`, emit loop `v4/src/v2_ops.rs:471-522` |
| `MEMO_DEPS` cache (forward only, no reverse index) | yes (forward) | `v4/src/runtime_graph.rs:125-145`, `:684-832` |
| `SourceIdentity { Git, Stat, Bytes }` | yes | `v4/src/source_index.rs:32-89` |
| `FactStoreClock` (durable only; no ephemeral clock) | yes | `v4/src/source_clock.rs:67-117` |
| VS Code extension wiring `.sprf` only | yes | `v4/editors/vscode/src/extension.ts:30-35`, `v4/editors/vscode/package.json:32-72` |
| Wide-selector extension at repo root | yes | `editors/vscode/src/extension.ts:28-39` |
| Watchman subscription via `watchman_client`, `VfsOverlay`, `WakeEvent` ingress | no | this plan |
| `MemoDepsCache::by_source` reverse index | no | covered by `v4/plans/lsp-fs-watcher-reactive-wake.md` "MEMO_DEPS reverse index" |
| daemon-side `DiagPublisher` (cross-URI publish) | no | new in this plan |
| extension declares non-`.sprf` document selectors for diag receipt | no | new in this plan |
| SCM-aware pre-warm via Watchman `since: scm.mergebase` | no | new in this plan |

Claude Code receives diagnostics via `mcp__ide__getDiagnostics(uri)`. That tool reads whatever the editor's diagnostics collection contains. Diagnostics arrive via standard LSP `publishDiagnostics` notifications from any connected LSP server the editor has wired up.

## 0.5 Mechanism choice: Watchman + `watchman_client`

This plan uses Facebook's Watchman daemon as the file-watching mechanism. Hand-rolled `notify::Watcher` is REJECTED as the default because:

- Linux `notify::RecursiveMode::Recursive` is a userspace walk + per-subdir `inotify_add_watch`. 250k subdirs = 250k syscalls on subscribe and ~270MB unswappable kernel RAM at 1080 B/watch.
- `fs.inotify.max_queued_events` defaults to 16384 and `git checkout` of a big diff overflows. Handling `IN_Q_OVERFLOW` requires a rescan trigger that Watchman implements and we would have to.
- Atomic-save lookalikes (VS Code `files.enableAtomicSave: true`, JetBrains safe-write) fire `IN_MOVED_TO` not `IN_MODIFY`. Naive watchers miss every save.
- Network mounts (NFS, SMB, WSL2 `/mnt/c`, Docker bind) deliver no events. Polling fallback is on us.

Watchman handles all of the above. It also gives us SCM-aware queries: `since: { scm: { mergebase-with: "main" } }` returns ONLY files changed since the merge-base. Pre-warm becomes "everything different from main," not "walk 50k files."

Trade-off: Watchman is an external C++ daemon. Users install it via `brew install watchman` or distro package; we ship a one-line check + helpful error on first connect. Already on most dev machines because Buck2, Mercurial, Jest, Metro, and Sapling all use it.

Rust client: the `watchman_client` crate (Facebook-maintained, on crates.io). Unix-socket BSER protocol; no Node.

Fallback (not in scope for this plan): if Watchman is unreachable, error out with installation instructions. A future opt-in `watchexec` fallback can be added later for distro-friendly installs.

## 1. End-to-end pipeline diagram

```
                    ┌──────────────────────────────────────────┐
                    │ Watchman daemon (external, brew install)│
                    │  - watch-project per workspace root      │
                    │  - subscribe per-root w/ since-clock     │
                    │  - settles VCS bursts via defer_vcs      │
                    └──────────────┬───────────────────────────┘
                                   │ BSER over unix socket
                                   ▼
                    ┌──────────────────────────────────────────┐
                    │ WatchmanIngress task (`watchman_client`) │
                    │  - next() per Subscription               │
                    │  - filter via paths_of_interest          │
                    │  - emit WakeEvent::FsChange              │
                    └──────────────┬───────────────────────────┘
                                   │ WakeEvent::FsChange
                                   ▼
   did_change (LSP) ──RPC──▶ ┌──────────────────────────────────────────┐
                             │ AppState::on_wake_event                   │
                             │  - resolve abs path                       │
                             │  - vfs.put / .remove                      │
                             │  - clocks.bump (durable | ephemeral)      │
                             │  - owners = memo_deps.by_source.get(sid)  │
                             │  - emit RuntimeJobs (warm_slice=[path])   │
                             └──────────────┬───────────────────────────┘
                                            │ mpsc::Sender<RuntimeJob>
                                            ▼
                             ┌─────────────────────────────────┐
                             │ DrainTask (50ms tick OR pull)   │
                             │  - try_recv batch               │
                             │  - dedupe on (owner, in_key)    │
                             │  - expand with warm_slice +     │
                             │    overlay-aware SourceReader   │
                             └──────────────┬──────────────────┘
                                            │ per (owner, uri) diag set
                                            ▼
                             ┌────────────────────────────────────┐
                             │ DiagPublisher (in sprefa-lsp)      │
                             │  - subscribe to AppState diag bus  │
                             │  - group by file URI               │
                             │  - throttle 50ms / coalesce        │
                             │  - tower_lsp::Client                │
                             │    .publish_diagnostics(uri, ...)   │
                             └──────────────┬─────────────────────┘
                                            │ JSON-RPC notification
                                            ▼
                             ┌────────────────────────────────────┐
                             │ VS Code / Cursor LSP client        │
                             │  - documentSelector: scheme=file   │
                             │    (no language filter, see §7)    │
                             │  - DiagnosticsCollection accepts   │
                             │    any URI                          │
                             └──────────────┬─────────────────────┘
                                            │
                                            ▼
                             ┌────────────────────────────────────┐
                             │ Claude Code IDE MCP server          │
                             │  mcp__ide__getDiagnostics(uri)     │
                             └──────────────┬─────────────────────┘
                                            ▼
                             Claude Code agent sees diagnostic
```

## 2. Type signatures (signatures first, bodies as comments)

### 2.1 `MemoDepsCache` reverse index — `v4/src/runtime_graph.rs:125-145`

REVISED 2026-05-20 after a two-reviewer design audit. The original draft proposed
`by_table` and `paths_of_interest`; both are removed. Rationale follows the type.

```rust
type OwnerKey  = String;   // owner_op_id = re_owner_hex (the seam digest at v2_ops.rs:1711)
type InKey     = String;   // in_key
type SidHex    = String;
type ContentHex= String;
type DepRow    = (SidHex, u64, ContentHex, PathBuf); // path canonical, see below

pub struct MemoDepsCache {
    /// KEEP. Existing forward shape (was: bare HashMap inside a Mutex<Option<...>>).
    forward: HashMap<(OwnerKey, InKey), Vec<DepRow>>,
    /// NEW. Reverse index for Phase 6's FsChange wake path. Built write-through
    /// from `forward`. Independent storage so Phase 6 reads do not serialize on
    /// the same lock that `record_memo_deps` grabs at expand-end. Stored
    /// alongside `forward` inside the same `MemoDepsCache` for code locality;
    /// the LOCK strategy is in §4.2.
    by_path: HashMap<PathBuf, HashSet<(OwnerKey, InKey)>>,
}

impl MemoDepsCache {
    /// Phase 6 hot path. O(1) point membership.
    pub fn contains_path(&self, p: &Path) -> bool;
    // body: self.by_path.contains_key(p)

    /// Phase 6 hot path. Returns an owned snapshot so the caller never holds
    /// the cache lock across the wake/mark_dirty/mpsc-send sequence.
    pub fn owners_of_path(&self, p: &Path) -> Vec<(OwnerKey, InKey)>;
    // body: self.by_path.get(p)
    //         .map(|s| s.iter().cloned().collect())
    //         .unwrap_or_default()

    /// Internal write-through. MUST diff-replace: the caller's `new_deps`
    /// is the COMPLETE set for `(owner, in_key)` (drain semantics at
    /// runtime_graph.rs:721 "this drain IS the complete set ... replace the
    /// cached entry wholesale"). So we compute `old_paths − new_paths` and
    /// prune the reverse buckets for the removed paths to avoid orphans.
    fn write_through(
        &mut self,
        key: &(OwnerKey, InKey),
        old: Option<&Vec<DepRow>>,
        new: &[DepRow],
    );
    // body:
    //   let new_paths: HashSet<&PathBuf> = new.iter().map(|(_,_,_,p)| p).collect();
    //   if let Some(old) = old {
    //     for (_,_,_,p) in old {
    //       if !new_paths.contains(p) {
    //         if let Some(set) = self.by_path.get_mut(p) {
    //           set.remove(key);
    //           if set.is_empty() { self.by_path.remove(p); }
    //         }
    //       }
    //     }
    //   }
    //   for (_,_,_,p) in new {
    //     self.by_path.entry(p.clone()).or_default().insert(key.clone());
    //   }
}
```

#### What was dropped and why

| dropped | reason |
|---|---|
| `by_table: HashMap<String, HashSet<...>>` | Table name is never carried in the deps tuple. `record_memo_deps` only receives `(SourceId, gen, content, path)`; the `SourceId` is a one-way blake3, the table name is unrecoverable. SQL mount call sites (`mounted_query.rs:430-431,514-515`) emit empty path AND empty source_path. No Phase 6 consumer either: Watchman wakes via FS paths, not table writes. |
| `owners_of_table` | follows from above |
| `paths_of_interest -> impl Iterator<Item=&Path>` | Borrowed-iterator across `.await` is `!Send`. Phase 6's actual need at plan line 281 is `contains(&Path) -> bool`. Enumeration is unused: the Watchman subscription filter is `Expr::Suffix`, not a per-path filter (per §2.3a line 260-261). |
| `record_write_through` "table branch" | falls out with `by_table` |

#### What the owner key actually is, and why no `owner_uri_id` mapping is needed

`OwnerKey = owner_op_id = re_owner_hex(row, kind)` is the SEAM digest (`v2_ops.rs:1711`), NOT a runtime-graph `OwnerNode.uri_id` (`runtime_graph.rs:249`). The codebase already accepts this mismatch: `mark_memo_owner_dirty(owner_hex, source_hex, gen)` at `runtime_graph.rs:994-1014` deliberately writes the RAW seam hex into the `owner_uri_id` column ("the worklist key is the same hex `MEMO_DEPS`/the seam use" — comment at lines 987-993). Phase 6's wake therefore routes through:

```
FsChange(path)
  -> owners_of_path(path) = Vec<(OwnerKey, InKey)>
  -> for each owner: mark_memo_owner_dirty(owner, source_hex, gen)
  -> existing sweep: dirty_memo_owners -> drain -> re-expand
```

No new wake path. Phase 5 plugs into the existing dirty machinery; the only new code is `by_path` population + lookup.

#### `source_path` canonicalization

`record_memo_deps` is called with `source_path: String` whose value is `abs.to_string_lossy().into_owned()` (`v2_ops.rs:862,3053`) or `path.to_string()` (`v2_ops.rs:1822`). NONE of those go through `dunce::canonicalize`. Watchman returns paths joined under a canonical root (e.g. macOS `/private/Users/...` vs our `/Users/...`). Without canonicalization, `by_path.contains(p)` MISSES every entry.

Phase 5 canonicalizes the `path` at the boundary of `write_through` (single seam) using the same `canon_path` already shipped in `vfs.rs` (`canon_path -> Option<PathBuf>` via `dunce::canonicalize`). If canonicalization fails (file deleted between write and stat), the entry is dropped from `by_path` but kept in `forward` — the disk truth for that source is gone, so a Phase 6 wake on that path would be a no-op anyway.

The PathBuf stored in `forward` keeps the canonical form too so `owners_of_path` and `flush_memo_deps` round-trip on the same key.

### 2.2 `VfsOverlay` — new struct in `v4/src/app.rs`

```rust
pub struct VfsOverlay {
    buffers: DashMap<PathBuf, BufferEntry>,
}

pub struct BufferEntry {
    pub text:    Arc<str>,
    pub version: i32,
    pub uri:     Arc<str>,  // original LSP URI (preserve scheme/auth)
}

impl VfsOverlay {
    pub fn new() -> Self;
    // body: Self { buffers: DashMap::new() }

    pub fn put(&self, path: PathBuf, uri: Arc<str>, text: Arc<str>, version: i32) -> PutOutcome;
    // body: match self.buffers.entry(path) {
    //   Occupied(mut e) if e.get().version >= version => PutOutcome::Stale,
    //   Occupied(mut e) => { e.insert(BufferEntry{...}); PutOutcome::Updated },
    //   Vacant(v)       => { v.insert(BufferEntry{...}); PutOutcome::Inserted },
    // }

    pub fn remove(&self, path: &Path) -> Option<BufferEntry>;
    // body: self.buffers.remove(path).map(|(_, e)| e)

    pub fn get_text(&self, path: &Path) -> Option<Arc<str>>;
    // body: self.buffers.get(path).map(|e| e.text.clone())

    pub fn is_owned(&self, path: &Path) -> bool;
    // body: self.buffers.contains_key(path)

    pub fn uri_for(&self, path: &Path) -> Option<Arc<str>>;
    // body: self.buffers.get(path).map(|e| e.uri.clone())
}

pub enum PutOutcome { Inserted, Updated, Stale }
```

### 2.3 `WakeEvent` — new enum, `v4/src/app.rs`

```rust
pub enum WakeEvent {
    /// Disk change reported by Watchman. The crate's `SubscriptionData::FilesChanged`
    /// is normalized here. `kind` is derived from the per-file `exists` + `new` flags
    /// returned by Watchman; we collapse to Modify | Remove | Create.
    FsChange { path: PathBuf, kind: FsKind },

    /// IDE buffer change. `text` overlays disk. `uri` preserved so
    /// the publish step can echo it back unchanged.
    BufferOpen   { path: PathBuf, uri: Arc<str>, text: Arc<str>, version: i32 },
    BufferChange { path: PathBuf, uri: Arc<str>, text: Arc<str>, version: i32 },
    BufferClose  { path: PathBuf, uri: Arc<str> },

    /// Watchman state-transition signal. Buck/Mercurial/etc. fire these
    /// around large operations ("hg.transaction", "hg.update", "git-checkout").
    /// We use them to pause expand until the working copy settles, then
    /// fold the resulting FilesChanged batch into one expand call.
    ScmStateEnter { state: Arc<str> },
    ScmStateLeave { state: Arc<str>, files_changed: Vec<PathBuf> },
}

pub enum FsKind { Modify, Remove, Create }
```

### 2.3a `WatchmanIngress` — new struct in `v4/src/app.rs`

```rust
pub struct WatchmanIngress {
    client:        watchman_client::Client,
    /// One subscription per workspace root resolved via `watch-project`.
    subs:          DashMap<PathBuf /* root */, SubscriptionHandle>,
    /// Tx half of the daemon's mpsc<WakeEvent>; cloned into each
    /// subscription's per-root tokio task.
    wake_tx:       mpsc::Sender<WakeEvent>,
    /// SCM clock snapshot stored per root for replay-after-restart.
    /// Persisted to `_watchman_clock` table on every successful drain.
    last_clock:    DashMap<PathBuf, watchman_client::Clock>,
}

struct SubscriptionHandle {
    sub:           watchman_client::Subscription<NameOnly>,
    task:          tokio::task::JoinHandle<()>,
    paths_filter:  Arc<DashSet<PathBuf>>, // snapshot of paths_of_interest at subscribe time
}

impl WatchmanIngress {
    pub async fn connect() -> anyhow::Result<Self>;
    // body: Connector::new().connect().await  -> client
    //       Self { client, subs: DashMap::new(), wake_tx, last_clock: DashMap::new() }

    pub async fn watch_root(&self, root: PathBuf, suffixes: &[&str]) -> anyhow::Result<()>;
    // body:
    //  1. let resolved = self.client.resolve_root(CanonicalPath::canonicalize(root)?).await?;
    //  2. let since = self.last_clock.get(&root).cloned();
    //  3. let (sub, _initial) = self.client.subscribe::<NameOnly>(
    //         &resolved,
    //         SubscribeRequest {
    //             expression: Some(Expr::Suffix(suffixes.iter().map(|s| s.into()).collect())),
    //             since: since.or_else(|| Some(Clock::ScmAware(ScmAwareClockData{
    //                 mergebase_with: Some("main".into()),
    //                 ..Default::default()
    //             }))),
    //             defer_vcs: false,        // we want vcs events
    //             defer: vec!["hg.update".into(), "git-checkout".into()],
    //             ..Default::default()
    //         }
    //     ).await?;
    //  4. spawn(self.drain_subscription(root, sub))
    //  5. self.subs.insert(root, handle)

    async fn drain_subscription(&self, root: PathBuf, mut sub: Subscription<NameOnly>) {
        // loop {
        //   match sub.next().await {
        //     Ok(SubscriptionData::FilesChanged(qr)) => {
        //       self.last_clock.insert(root.clone(), qr.clock.clone());
        //       for f in qr.files.unwrap_or_default() {
        //         // f.name: PathBuf relative to root
        //         let abs = root.join(&f.name);
        //         if !self.paths_of_interest_contains(&abs) { continue; }
        //         let kind = if f.exists { if f.new { FsKind::Create } else { FsKind::Modify } }
        //                    else { FsKind::Remove };
        //         let _ = self.wake_tx.send(WakeEvent::FsChange{ path: abs, kind }).await;
        //       }
        //     }
        //     Ok(SubscriptionData::StateEnter { state_name, .. }) =>
        //       self.wake_tx.send(WakeEvent::ScmStateEnter{ state: state_name.into() }).await.ok(),
        //     Ok(SubscriptionData::StateLeave { state_name, .. }) =>
        //       /* coalesce the resulting FilesChanged into one ScmStateLeave */,
        //     Err(_) => /* reconnect with backoff */,
        //   }
        // }
    }

    pub async fn scm_changed_since_main(&self, root: &Path) -> anyhow::Result<Vec<PathBuf>>;
    // body: query with since: Clock::ScmAware { mergebase_with: "main" };
    //       returns the file list that differs from main's merge-base.
    //       Used by pre-warm (§8).
}
```

### 2.4 `DiagPublisher` — new in `v4/crates/sprefa-lsp/src/main.rs`

```rust
struct DiagPublisher {
    client:   Client,                            // tower_lsp::Client
    sprf:     Arc<dyn SprfClient>,
    /// last published per-uri count + content hash. Lets the publisher
    /// skip a notification when nothing changed, and skip a clear when
    /// previous publish was already empty.
    last:     Mutex<HashMap<Url, (usize, u64)>>,
    /// throttle window — coalesce per-uri publish bursts.
    throttle: Duration,
}

impl DiagPublisher {
    pub fn new(client: Client, sprf: Arc<dyn SprfClient>) -> Self;
    // body: Self { client, sprf, last: Mutex::new(HashMap::new()),
    //              throttle: Duration::from_millis(50) }

    /// One-shot for a known set of (URI, diags). Called by the drain
    /// task after each expand.
    pub async fn publish_set(&self, set: Vec<(Url, Vec<Diagnostic>)>);
    // body: for (uri, diags) in set {
    //         let hash = hash_diags(&diags);
    //         let mut g = self.last.lock().await;
    //         match g.get(&uri) {
    //           Some((_, h)) if *h == hash => continue,
    //           _ => g.insert(uri.clone(), (diags.len(), hash)),
    //         }
    //         drop(g);
    //         self.client.publish_diagnostics(uri, diags, None).await;
    //       }

    /// Fetch all URIs the daemon has produced diags for since `since_gen`
    /// and publish them. Used at initialize for cold pre-warm and on
    /// FsChange-fired wake.
    pub async fn refresh_all(&self, since_gen: u64) -> u64;
    // body: 1. RPC: sprf.list_diagnostic_uris_since(since_gen) -> Vec<String>
    //       2. for uri in list { publish_set(vec![(uri, get_diags(uri))]) }
    //       3. return new high-watermark gen
}
```

### 2.5 New RPCs (extend `sprf_rpc!` block in `v4/src/app.rs:470-485`)

```rust
sprf_rpc! {
    // ... existing ...

    /// Buffer-side overlay notifications (uri → abs path resolution
    /// happens daemon-side via `Url::to_file_path`).
    fn lsp_buffer_open   (LspBufferOpenReq)   -> LspBufferAck => "/lsp/buffer/open";
    fn lsp_buffer_change (LspBufferChangeReq) -> LspBufferAck => "/lsp/buffer/change";
    fn lsp_buffer_close  (LspBufferCloseReq)  -> LspBufferAck => "/lsp/buffer/close";

    /// Disk-side notification (LSP server forwards from its `notify`
    /// task once mode is "frontend watcher"; or daemon-internal once
    /// the daemon owns the watcher).
    fn lsp_fs_change     (LspFsChangeReq)     -> LspFsChangeResp => "/lsp/fs/change";

    /// Pull list of URIs the daemon has unpublished diags for since
    /// `since_gen`. Used by DiagPublisher.refresh_all.
    fn lsp_diags_dirty   (LspDiagsDirtyReq)   -> LspDiagsDirtyResp => "/lsp/diags/dirty";
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspBufferOpenReq    { pub uri: String, pub text: String, pub version: i32 }
pub type LspBufferChangeReq    = LspBufferOpenReq;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspBufferCloseReq   { pub uri: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspBufferAck        { pub gen: u64 }   // wake gen after this event
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspFsChangeReq      { pub path: String, pub kind: String /* "modify"|"create"|"remove" */ }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspFsChangeResp     { pub gen: u64, pub woken_owners: u32 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspDiagsDirtyReq    { pub since_gen: u64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspDiagsDirtyResp   { pub uris: Vec<String>, pub high_watermark: u64 }
```

### 2.6 VS Code extension `package.json` extension (declarative)

```jsonc
{
  // existing "activationEvents": ["onLanguage:sprf"]  must broaden.
  "activationEvents": [
    "onLanguage:sprf",
    "workspaceContains:**/*.sprf"   // wake on any .sprf in workspace
  ],
  // new languages registered so VS Code's diagnostic collection
  // accepts non-.sprf URIs from this server (see §6 + §7).
  "contributes": {
    "languages": [
      { "id": "sprf",  "extensions": [".sprf"] }
      // diagnostics-only document selectors expand to:
      //  rust, typescript, javascript, python — superset of v4 lints.
    ]
  }
}
```

```ts
// extension.ts shape (no path string yet — placement is §13)
interface SprfDiagsClientOptions {
    documentSelector: DocumentSelector;     // see §7
    fileEvents: FileSystemWatcher[];        // .sprf only
    middleware: Middleware;                 // narrow didOpen/didChange to .sprf
}
function buildClient(opts: SprfDiagsClientOptions): LanguageClient;
```

### 2.7 Pre-warm walker — `v4/src/app.rs`

```rust
pub struct PreWarmConfig {
    pub root:           PathBuf,
    pub globs:          Vec<String>,        // from sprf rule globs once compiled
    pub max_files:      usize,              // hard cap, default 50k
    pub budget:         Duration,           // wall budget, default 5s
}

impl AppState {
    pub async fn pre_warm(&self, cfg: PreWarmConfig) -> PreWarmReport;
    // body:
    //  1. WalkBuilder::new(cfg.root).git_ignore(true).build()
    //  2. for each path matching cfg.globs (up to cfg.max_files or budget):
    //       - push WakeEvent::FsChange { path, kind: Modify }
    //       - count
    //  3. return PreWarmReport { walked, queued, deadline_hit }
}

#[derive(Debug)]
pub struct PreWarmReport {
    pub walked:         usize,
    pub queued:         usize,
    pub deadline_hit:   bool,
}
```

### 2.8 Drain task signature — `v4/src/app.rs`

```rust
pub async fn run_drain_loop(
    state:     Arc<SprfState>,
    rx:        mpsc::Receiver<RuntimeJob>,
    publisher: Arc<dyn DiagSinkOut>,         // trait-erased DiagPublisher
);
// body:
//  loop {
//    let mut batch = vec![rx.recv().await?];
//    while let Ok(more) = rx.try_recv() { batch.push(more); }
//    batch.sort_by_key(|j| (j.owner_op_id.clone(), j.in_key.clone()));
//    batch.dedup_by(|a, b| a.owner == b.owner && a.in_key == b.in_key);
//    let warm = collect_warm_paths(&batch);
//    let diags_by_uri = state.expand_with_warm(warm).await?;
//    publisher.publish_set(diags_by_uri).await;
//  }
```

### 2.9 `DiagSinkOut` trait — bridges daemon → LSP frontend

```rust
#[async_trait]
pub trait DiagSinkOut: Send + Sync {
    async fn publish_set(&self, set: Vec<(String /* uri */, Vec<SprfDiag>)>);
}
```

LSP frontend impl wraps `DiagPublisher`. Daemon-only run mode uses a no-op impl (CLI never publishes).

## 3. Instance lifetimes

| type | owner | lifetime | instance count |
|---|---|---|---|
| `SprfState` | global / process | daemon process | 1 |
| `VfsOverlay` | `SprfState` | daemon process | 1 |
| `MemoDepsCache` | `RuntimeGraph` (inside `SprfState`) | daemon process | 1 |
| `WakeEvent` | transient | per event | unbounded throughput; size-bounded via mpsc cap 1024 |
| `DiagPublisher` | `Backend` in `sprefa-lsp` | one per LSP session | 1 per `LspService::new` |
| `Backend.docs` cache | `Backend` | LSP session | 1 |
| `notify::Watcher` | `FsWatcherState` (inside `SprfState`) | daemon process | 1 |
| `notify-debouncer-mini` task | tokio task | daemon process | 1 |
| `DrainTask` | tokio task | daemon process | 1 |
| `BufferEntry` | `VfsOverlay.buffers` shard | from `did_open` to `did_close` | one per open file |
| `Backend.sprf` `Arc<dyn SprfClient>` | `Mutex` in `Backend` | swapped on workspace-root change | 1 |
| `PreWarmConfig` | `Backend::initialize` | one initialize call | drop after pre_warm returns |
| VS Code `LanguageClient` | extension `activate` | until `deactivate` | 1 |

## 4. Storage layout, reads, writes, uniqueness

### 4.1 `VfsOverlay.buffers` (in-RAM)
- key: `PathBuf` (canonicalized via `Url::to_file_path` then `dunce::canonicalize`).
- value: `BufferEntry { text: Arc<str>, version: i32, uri: Arc<str> }`.
- index: native DashMap shard hash on `PathBuf`.
- read path: `SourceReader::read_cursor_with_intern` (hot path).
- write path: `on_wake_event(BufferOpen | BufferChange)` (linearized via DashMap shard).
- uniqueness: one entry per absolute path; `put` returns `PutOutcome::Stale` if `version <= existing`.

### 4.2 `MemoDepsCache.by_path` (in-RAM, write-through, diff-replace)
- key: `PathBuf`, canonicalized via `vfs::canon_path` (`dunce::canonicalize`).
- value: `HashSet<(OwnerKey, InKey)>`. Empty buckets are removed; `contains_path` is therefore "has any owner".
- index: HashMap.
- read path: `AppState::on_wake_event` calls `owners_of_path(p) -> Vec<...>` (owned snapshot, no lock held across await).
- write path: `record_memo_deps` extended at `v4/src/runtime_graph.rs:730-767`. MUST read prior `forward.get(&key)` BEFORE the `insert`, pass `(old, new)` to `write_through`, then overwrite forward. Singular `record_memo_dep` (line 684-713) also routes through `write_through` (it's a public API; can't be left as a back door).
- uniqueness: `(OwnerKey, InKey)` set semantics per bucket.
- cold-load: `memo_deps_loaded` (line 775-832) builds `forward` from sqlite on first access; the same loop calls `write_through(key, None, &set)` to seed `by_path`. Daemon caveat: when `compact_sources.is_some()` (the LSP daemon's typical mode, line 788-794), the load is skipped and `by_path` is populated only by expand-time writes. Acceptable; the pre-warm path (§3 Phase 3) is the population mechanism on cold daemon start, and a first wake before any expand is a no-op (correct: there are no recorded deps).
- locking: stored inside the same `Arc<Mutex<Option<MemoDepsCache>>>` as `forward` (one allocation, one lock). Phase 6 reads via `owners_of_path` clone the set under the lock and release before mark_dirty/mpsc-send; Phase 5 imposes no async lifetime. If contention shows up in Phase 6 profiling, split `by_path` into its own `RwLock` then.

### 4.3 (deleted)
`by_table` is removed from the plan; see §2.1 "What was dropped and why".

### 4.4 `pending_jobs` mpsc
- bounded cap 1024.
- sender cloned into FsWatcher task + each RPC handler.
- receiver in `DrainTask`.
- backpressure: a full channel blocks the FsWatcher task; LSP RPC handlers go via `try_send` and return `LspBufferAck { gen: 0 }` on overflow (frontend retries via the version checkpoint).

### 4.5 `DiagPublisher.last`
- key: `Url`.
- value: `(count, blake3_64_of_diag_payload)`.
- size: one entry per URI ever published; bounded by `paths_of_interest`.
- read path: per `publish_set` call.
- write path: same call.
- uniqueness: one row per URI; insert-or-update.

### 4.6 `Clocks` (new struct)
- `Clocks.durable`: existing `FactStoreClock` (sqlite-backed).
- `Clocks.ephemeral`: in-RAM `DashMap<SourceId, AtomicU64>`.
- read path: `Clocks::current_gen(sid) = max(durable, ephemeral)`.
- write path: `bump_durable(sid)` on `FsChange | BufferClose`; `bump_ephemeral(sid)` on `BufferOpen | BufferChange`.

### 4.7 `_memo_deps` sqlite table (existing, no schema change)
- key: `(owner_op_id, in_key, source_id)` PK — `v4/src/runtime_graph.rs:669-676`.
- read at daemon start (one `rows_of`).
- write at expand end (one batched `flush_memo_deps`).

## 5. Reactivity model — two clocks, Watchman-sourced disk events

| event source | clock bumped | dedup | win window |
|---|---|---|---|
| `did_change(uri, text, v)` | `ephemeral(sid)` | `pending` map in `Backend` (`v4/crates/sprefa-lsp/src/main.rs:131-189`) | 80ms keystroke debounce |
| Watchman `FilesChanged` | `durable(sid)`, only if `SourceIdentity` differs | Watchman-side settle + our per-batch `BTreeSet` coalesce | per-subscription, set by Watchman defaults |
| `did_save` | none (file write fires Watchman separately) | — | — |
| `did_close(uri)` | `durable(sid)` | drop `vfs` entry then treat as FsChange | immediate |

The `durable` clock is the sqlite-backed `FactStoreClock`. The `ephemeral` clock is in-RAM, bumped on every `did_change`. Watchman's own clock token is stored separately in `WatchmanIngress.last_clock` and persisted via `_watchman_clock` table on each successful drain so we resume from the right point after a daemon restart.

### 5.1 Dedup when both fire (typical: VS Code "auto-save on focus loss" + keystroke)
- Sequence: `did_change` → 80ms debounce → daemon receives `BufferChange` → `ephemeral.bump(sid)`. Save happens. Watchman fires `FilesChanged`.
- Daemon sees `FsChange`. `vfs.is_owned(path) == true` (still owned). Short-circuit: no clock bump, no job emit.
- When `did_close` arrives, `vfs.remove(path)` + `durable.bump(sid)`. New `expand` runs against on-disk text (now identical to the buffer; no work to do besides confirming).

### 5.2 Git-checkout burst (Watchman-handled)
- `git checkout` rewrites 1000 files in <100ms.
- `defer: ["hg.update", "git-checkout"]` in the `SubscribeRequest` tells Watchman to PAUSE delivery while the working copy is mid-transition. We receive a `StateEnter("git-checkout")` event, then a single `StateLeave` carrying the full coalesced `FilesChanged` set after the working copy settles.
- `on_wake_event(ScmStateEnter)` flips an `is_settling` flag; the drain task stops emitting RuntimeJobs.
- `on_wake_event(ScmStateLeave)` clears the flag and emits one big batch to the drain task. ONE expand call per checkout, not 1000.
- The `IN_Q_OVERFLOW` failure mode of hand-rolled inotify does not exist: Watchman's daemon keeps its own state and re-emits the consistent diff on resubscribe. Crash-recovery is by clock-token replay.

### 5.3 Out-of-band events (gitignored writes, `target/` churn, etc.)
- Watchman honors `.watchmanconfig` ignore globs at the watch-project level.
- We ship a `_sprf_watchman_config.json` template that excludes `.git/`, `target/`, `node_modules/`, `dist/`, `build/`, `.next/`, `__pycache__/`, `.venv/`.
- `setup` on `initialize` writes this template iff no `.watchmanconfig` exists in the workspace root. Existing user config is left untouched.

## 6. Cross-URI diagnostic publish

### 6.1 tower-lsp side
- `Client::publish_diagnostics(uri, diags, version)` at `tower-lsp-0.20.0/src/service/client.rs:350-360` just sends `PublishDiagnosticsParams::new(uri, diags, version)` as a notification. NO check that the URI was previously seen via `did_open`. The spec language says "an open file" but tower-lsp does not enforce this. Confirmed by reading the source.

### 6.2 VS Code LSP client side
- `vscode-languageclient` accepts any URI in the params. The diagnostics get pushed into VS Code's `DiagnosticCollection` for the server. They appear in "Problems" panel and `vscode.languages.getDiagnostics(uri)` returns them.
- Caveat: the editor will show inline squiggles only if a `TextDocument` for that URI is currently open. Otherwise the diagnostic still lives in the collection and `mcp__ide__getDiagnostics(uri)` (which goes through `vscode.languages.getDiagnostics`) returns it.
- **Open question (§12.3):** confirm `vscode-languageclient` does not filter by `documentSelector` on the inbound `publishDiagnostics` path. The selector controls outbound (which docs the server hears about); inbound diagnostic delivery is typically unconditional. Verify by reading the client lib.

### 6.3 Claude Code MCP behavior
- `mcp__ide__getDiagnostics(uri)` reads the editor's diagnostic collection. If the editor accepted the publish, the agent sees it. If `uri` is omitted, the agent gets all diagnostics across the workspace.

### 6.4 Implication
- Plan: have sprefa-lsp publish `Rust file URIs` (or `.ts` URIs, etc.) for diagnostics raised by `.sprf` lint programs that target those files. The IDE must accept those URIs.
- Required: the VS Code extension must declare a `documentSelector` broad enough that VS Code does not silently drop the diagnostics. Spec-conformant clients accept any URI; production VS Code does too. **The wide-selector extension at `editors/vscode/src/extension.ts:28-39` already does this** — it declares `[{ scheme: 'file' }]` (no language filter). The `v4/editors/vscode/` variant at `:30-35` is narrower (`language: 'sprf'`) and must be widened. Plan §7 details which variant to keep.

## 7. VS Code / Cursor extension wiring

### 7.1 Two candidate variants in the repo

| variant | path | selector | publish scope |
|---|---|---|---|
| narrow | `v4/editors/vscode/` | `{ scheme: 'file', language: 'sprf' }` | rejects diagnostics on .rs URIs in some client versions |
| wide   | `editors/vscode/`    | `{ scheme: 'file' }` + middleware filter for didOpen/didChange | accepts diagnostics on any file URI |

### 7.2 Pick wide

The wide variant at `editors/vscode/src/extension.ts` is the correct shape. The middleware narrows OUTBOUND sync notifications (only push `.sprf` `didOpen`/`didChange` into the server) without limiting INBOUND `publishDiagnostics` URI acceptance.

### 7.3 Required edits (in the wide variant)

```ts
const clientOptions: LanguageClientOptions = {
  // Accept inbound diagnostics for ANY file URI.
  documentSelector: [{ scheme: 'file' }],
  // .sprf-only outbound buffer sync.
  synchronize: {
    fileEvents: workspace.createFileSystemWatcher('**/*.sprf'),
  },
  middleware: {
    didOpen:   (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
    didChange: (e,   next) => isSprf(e.document.uri) ? next(e) : Promise.resolve(),
    didClose:  (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
    didSave:   (doc, next) => isSprf(doc.uri) ? next(doc) : Promise.resolve(),
  },
};
```

### 7.4 Activation events

Add `workspaceContains:**/*.sprf` to `activationEvents` (today only `onLanguage:sprf` at `package.json:29-31`). Without this, a workspace where no `.sprf` file has been opened yet keeps the server dormant and Claude Code sees zero diagnostics.

### 7.5 stdio vs socket

Keep stdio. Rationale: VS Code spawns one server per workspace; stdio is the lowest-overhead transport and matches the existing wiring at `v4/editors/vscode/src/extension.ts:26-28`. Socket transport would only be needed for a multi-client daemon, which `SPREFA_LSP_DAEMON_URL` already covers via HTTP at `v4/crates/sprefa-lsp/src/main.rs:84-92`.

### 7.6 Cursor / other LSP-compliant editors

Cursor uses the same VS Code extension. No separate wiring. The Claude Code CLI's `mcp__ide__*` is editor-agnostic; it reads diagnostics through whatever IDE bridge it connects to.

## 8. Pre-warm on `initialize` (SCM-aware via Watchman)

Cold-start problem: with no `_memo_deps` (first run) and no buffer events, `paths_of_interest` is empty. Outside-IDE edits cannot wake. Worse, Claude Code asking `mcp__ide__getDiagnostics` immediately sees nothing.

### 8.1 Strategy: SCM-aware Watchman query

On `initialize`, for each workspace root:

```rust
let qr = client.query::<NameOnly>(&resolved_root, QueryRequestCommon {
    expression: Some(Expr::Suffix(vec!["rs".into(), "ts".into(), "py".into(), "sprf".into()])),
    since: Some(Clock::ScmAware(ScmAwareClockData {
        mergebase_with: Some("main".into()),
        ..Default::default()
    })),
    ..Default::default()
}).await?;
// qr.files: every file under root whose contents differ from the
// merge-base with main.
```

This returns ONLY files that are modified, added, or removed relative to `main`. On a freshly-checked-out branch with 5 changed files this returns 5 paths. On a 500-repo workspace where most submodules are at HEAD, this returns the small subset that diverges.

Push each returned path as `WakeEvent::FsChange { kind: Modify }`. DrainTask runs expand on the batch.

### 8.2 50k-file single repo
- SCM-aware query is one Unix-socket round-trip; Watchman has the answer cached from its own watch state.
- Typical result size on a working branch: 10-500 paths.
- Pre-warm wall time: ~50ms (one round-trip + N FsChange ingests).
- Compare to the rejected `WalkBuilder` approach: 5s budget, 50k file walk. SCM-aware wins by two orders of magnitude.

### 8.3 500-repo workspace (monorepo of submodules)
- One `watch-project` + one SCM query per repo that contains `.sprf` rules.
- Skip repos with no `.sprf` file via a `find ./*/. -name '*.sprf'` pre-pass (or one Watchman `query` against the workspace umbrella root if Watchman is rooted there).
- Per-repo wall: ~50ms. 100 repos with rules = 5s, parallelisable to <1s with `futures::join_all`.

### 8.4 Initial-cold case (first time on a branch, no `main` reference)
- If `mergebase_with: "main"` fails (Watchman returns an error: no merge-base), fall back to `since: Clock::Spec(ClockSpec::null_clock())` which returns ALL files matching the suffix expression. This IS the full walk; cap at `max_files=50_000` and budget `5s` as a safety.
- Cache the resulting clock token in `_watchman_clock` so subsequent boots resume from that point.

### 8.5 Lazy alternative (rejected as default)
Skip pre-warm entirely; rely on `did_open` for the user's currently-open file. Rejected because Claude Code's whole value is asking about files the user is NOT looking at. The SCM-aware pre-warm is so cheap that this trade-off no longer matters.

## 9. Phasing / build order

### Phase 1 — wide-selector extension swap
- Edit `v4/editors/vscode/src/extension.ts` to match the wide variant at `editors/vscode/src/extension.ts:28-39` (selector `[{ scheme: 'file' }]`, middleware-narrowed outbound sync).
- Add `workspaceContains:**/*.sprf` to `v4/editors/vscode/package.json` activation events.
- No daemon changes.
- **User-visible outcome at this checkpoint:** opening a `.sprf` file in VS Code triggers diagnostics (already works today); but the agent now ALSO sees them via `mcp__ide__getDiagnostics`, because the selector no longer narrows inbound delivery. First .sprf diag visible to Claude Code agent.

### Phase 2 — cross-URI publish (no FS watcher yet)
- Extend `lsp_error(:rule_id)` so that when its emitted `Diag` carries an `FS` span pointing at a NON-.sprf file (e.g., a .rs file via the n+1 lint), the daemon attaches that file's URI to the diag, not the .sprf URI. Path resolution via the existing `c.get("FS")` cursor field.
- Extend `SprfDiag` with `pub uri: Option<String>` field. Default `None` = the requesting `.sprf` URI; `Some(p)` = a different file URI.
- LSP frontend groups diags by URI before publishing.
- New RPC: `lsp_diags_by_uri(req: LspDiagsByUriReq) -> HashMap<String, Vec<SprfDiag>>`.
- **User-visible outcome:** open a `.sprf` lint program that targets `.rs` files; diagnostics appear on the `.rs` files in VS Code Problems panel AND `mcp__ide__getDiagnostics("file:///.../target.rs")` returns them. No reactive update yet; diags refresh on `.sprf` keystroke only.

### Phase 3 — pre-warm walker
- Implement `AppState::pre_warm` (§2.7).
- Call from `Backend::initialize` after `set_workspace_root` (`v4/crates/sprefa-lsp/src/main.rs:196-199`).
- DiagPublisher polls `lsp_diags_dirty` once after pre-warm completes; publishes union of URIs.
- **User-visible outcome:** with no `.sprf` files open, Claude Code agent calling `mcp__ide__getDiagnostics()` immediately after VS Code launch sees the full set of lint diagnostics on `.rs`/`.ts` files. Cold-start latency: ~5s for 50k files.

### Phase 4 — VfsOverlay + buffer-vs-disk path
- Implement `VfsOverlay` and `WakeEvent::BufferOpen | BufferChange | BufferClose`.
- Thread `Arc<VfsOverlay>` into `SourceReader` (`v4/src/source.rs:40`).
- `Backend::refresh` no longer pushes through `lsp_open`/`lsp_change`; it pushes through `lsp_buffer_open`/`lsp_buffer_change`. Existing RPCs stay for backward compat (kept as thin wrappers).
- **User-visible outcome:** edits to a `.sprf` file in VS Code immediately re-lint with the unsaved buffer contents; agent sees those buffer-based diags via `mcp__ide__getDiagnostics`.

### Phase 5 — `MemoDepsCache::by_path` reverse index (REVISED 2026-05-20)
- Wrap the existing `memo_deps_cache` field in a `MemoDepsCache` struct per §2.1 (forward + by_path; NO by_table).
- Write-through `write_through(key, old, new)` called from THREE sites:
  - `record_memo_deps` (`v4/src/runtime_graph.rs:730-767`): the batched drain-end site.
  - `record_memo_dep` (line 684-713): the singular site.
  - `memo_deps_loaded` cold-load loop (line 775-832): seeds `by_path` from sqlite-loaded forward map. Skipped on `compact_sources.is_some()` (daemon mode).
- Canonicalize paths at the `write_through` boundary using existing `vfs::canon_path` (single seam).
- Drop empty-`source_path` entries from `by_path`; keep them in `forward` so the existing `memo_dep_owners_for_source` sweep still finds them via SourceId.
- Phase 6 will route: `FsChange(p)` → `owners_of_path(canon_path(p))` → for each `(owner, in_key)` call existing `mark_memo_owner_dirty(owner, source_hex, gen)`. No new wake path; Phase 5 just plugs into `runtime_graph.rs:994-1014`.
- No watcher yet.
- **User-visible outcome:** zero direct user effect. `contains_path(p)` and `owners_of_path(p)` are now O(1). Internal precondition for Phase 6.

### Phase 6 — WatchmanIngress + drain task (REVISED 2026-05-20, MVP-6a scope)

A two-reviewer audit (notes appended below) found that the original Phase 6 design routed wakes through `mark_memo_owner_dirty` + `dirty_memo_owners`, which the daemon NEVER reads. The actual reactive arc the daemon uses is:

| step | code site |
|---|---|
| `ast` / `read` write `SubscribeEdge(owner → file_dirty_source_uri(path))` | `v2_ops.rs:820, 887, 3013-3016` |
| `RuntimeGraph::dispatch_source_wake(SourceWake)` ⇒ `incoming_subscribers` ⇒ core mark-dirty | `runtime_graph.rs:1604-1620, :1808-1828` |
| `SprfState::drain_ready` ⇒ `drain_runtime_until_idle` ⇒ `drain_graph_jobs` (GraphReplayRunner) | `app.rs:791-826` |

Phase 5's `MemoDepsCache::by_path` + `mark_memo_owner_dirty` is **parallel infrastructure**, not on the daemon path: `dirty_memo_owners` is only consumed by `DirtySourceDriver::sweep_to_quiescence` (`dirty_source.rs:139-178`), and that sweep has zero non-test callers. Phase 5 stays (working code, tests pass) as parked infrastructure for a future "wake fewer than all open .sprf docs" optimization but is NOT on the MVP-6a critical path.

#### MVP-6a (this slice)

- New file `v4/src/wake.rs`: `WakeEvent::FsChange { paths: Vec<PathBuf> }`. ONLY this variant. No buffer events here (Phase 4's RPCs already cover them); no SCM state events (Phase 6b).
- `tokio::sync::mpsc::channel(1024)` from `WatchmanIngress` subscriber task to `SprfState`-owned drain task.
- `WatchmanIngress::start(root, suffixes, tx)`: connects, `resolve_root`, `subscribe::<NameOnly>` with `Expr::Suffix([".sprf", ".rs", ".ts", ".tsx", ".js", ".jsx", ".py"])`, spawns a tokio task that translates `SubscriptionData::FilesChanged` into ONE `WakeEvent::FsChange { paths }`. NO per-file fanout (each Watchman batch is one mpsc send).
- Drain task lives in `SprfState`; consumes `WakeEvent::FsChange { paths }`:
  ```
  for path in paths:
      sid = SourceId::for_file(path.to_string_lossy())   // RAW path, no canonicalize (matches v2_ops.rs:838 recorder)
      gen = clock.bump(sid)
      graph.dispatch_source_wake(SourceWake::dirty(file_dirty_source_uri(&path), gen))
  drain_ready()
  // Re-publish: re-run lsp_change for every open .sprf doc (typical: 1-5)
  ```
- LSP frontend wires `Backend::initialize` to call a new `lsp_start_watch` RPC (or piggyback on `lsp_pre_warm`'s existing watchman_ok flag) and spawns the drain task in the LSP-side backend.
- **User-visible outcome:** `std::fs::write("foo.rs", ...)` from outside the IDE wakes any `.sprf` lint that depended on `foo.rs`; agent sees the resulting diag via `mcp__ide__getDiagnostics` within ~100ms of the write.

#### Why no `Clocks::ephemeral` split (review #1 finding 4)

The seam's staleness oracle is `SourceIdentity` (git-OID / stat tuple) compared content-hash-wise; `SourceClock::current_gen` is consulted only by the SQL cache key (`v4/src/sql.rs:1525`). Splitting `durable` vs `ephemeral` buys nothing the current architecture cares about. Drop from the plan; keep ONE `FactStoreClock`.

#### Why no `vfs.is_owned` suppression (review #1 finding 5)

`SourceId::for_buffer` has ZERO callsites outside its own tests; the live read path uses `SourceId::for_file(path)` regardless of overlay presence. Save-then-Watchman-wake on an overlay-owned path is harmless: the next expand reads overlay bytes; `SourceIdentity` of the disk file moved; staleness fires; refresh runs once over overlay text. The "double refresh" cost is one expand; the suppression's race window (save → buffer-close in <50ms drops the disk wake) is worse than the redundant refresh.

#### Window/showMessage on connect failure (review #1 finding 6)

`SprfState` has no `tower-lsp::Client` handle. Phase 6 reuses `LspPreWarmResp.watchman_ok: bool` (already in `app.rs`) and adds an optional `watchman_hint: Option<String>`; LSP frontend (`v4/crates/sprefa-lsp/src/main.rs`) calls `self.client.show_message(...)` when `watchman_ok == false`.

#### Deferred to Phase 6b

- `WakeEvent::ScmStateEnter { state } | ScmStateLeave { state }` (no `files_changed` field — review #1 finding 9: Watchman delivers the settle batch as a separate `FilesChanged` after `StateLeave`).
- `defer: ["hg.update", "git-checkout"]` SCM-aware settle.
- `_watchman_clock` table for replay-after-restart.
- Reconnect backoff (250ms → 30s).
- Batch coalescing window (per-batch is already coalesced by Watchman; rapid-edit-tool case is open).

### Phase 7 — `DiagPublisher.last` dedup + reconnect hardening
- Implement publish-side throttle window (default 50ms) and `last` hash-skip.
- Reconnect backoff for Watchman socket loss (250ms, 500ms, 1s, 2s, max 30s); resubscribe with persisted `last_clock`.
- Test the daemon-restart path: kill `watchman` mid-session, assert ingress reconnects and replays the diff.
- **User-visible outcome:** `git checkout` already handled by Phase 6's `defer` settle; Phase 7 hardens the publish-side dedup so that idempotent re-publishes don't spam the editor across reconnects.

## 10. Tests

### Phase 1
- `tests/lsp_wide_selector_publish.rs`: spawn server, simulate VS Code initialize with `documentSelector=[{scheme:file}]`. Send a synthetic `publishDiagnostics` for a `.rs` URI. Assert that diagnostics appear in the collection (mock client harness).

### Phase 2
- `tests/lsp_cross_uri_diag.rs`: open `dogfood-sprf-component-n-plus-1.sprf` (the worked case from `v4/plans/lsp-sprf-component-n-plus-1-lint.md`); assert `get_diags` returns a `SprfDiag` whose `uri` is the `.rs` fixture path, not the `.sprf` path.
- `tests/lsp_diags_by_uri.rs`: same setup; assert the new RPC returns a `HashMap` with the `.rs` URI as one key.

### Phase 3
- `tests/lsp_pre_warm_walks.rs`: tempdir with `lint.sprf` (a glob over `**/*.rs`) and 100 `.rs` fixtures. Call `pre_warm`; assert all 100 files are visited and `get_diags` returns diagnostics on each.
- `tests/lsp_pre_warm_budget.rs`: same with `max_files=50`; assert `deadline_hit=false` and exactly 50 files visited.

### Phase 4
- `tests/lsp_overlay_supersedes_disk.rs`: open buffer with text differing from disk; assert `expand` reads buffer text (diag count matches buffer, not disk).
- `tests/lsp_overlay_close_resumes_disk.rs`: open buffer, edit it to a state with no diag; close; assert disk-content diags reappear.

### Phase 5
- `tests/memo_deps_by_path_reverse_index.rs`:
  1. **record-and-lookup**: record `(owner=A, in_key=X)` with paths `{p1, p2}`; `owners_of_path(p1) == [(A,X)]`, `contains_path(p1) == true`.
  2. **diff-replace drops orphans**: re-record `(A, X)` with paths `{p2, p3}`; `owners_of_path(p1)` empty, `contains_path(p1) == false`, `owners_of_path(p2) == [(A,X)]`, `owners_of_path(p3) == [(A,X)]`.
  3. **cold-load seeds by_path** (in-memory FactStore path, NOT compact_sources): pre-populate `MEMO_DEPS` via `facts.insert`, force `memo_deps_loaded()`, assert `owners_of_path(p)` returns the recorded owners before any record call. Skips on the `compact_sources.is_some()` daemon mode (asserts that path stays empty until expand).
  4. **canonicalization symmetry**: on macOS, write a path under `tempfile::tempdir()` (resolves to `/var/folders/...`), record it via `record_memo_deps`, then look up via `dunce::canonicalize(p)` (yields `/private/var/folders/...`). MUST hit.
  5. **empty-path / SQL-mount entry**: `record_memo_deps` called with a row whose `source_path` is `""` (SQL mount, mounted_query.rs:430-431); assert `forward` still contains it (so the dirty sweep can find it via `memo_dep_owners_for_source`) but `by_path` does NOT get an empty-PathBuf key.
  6. **singular `record_memo_dep` write-through**: call the singular API once per source for a key; assert `by_path` has one bucket per non-empty path with the (owner, in_key) inside.

### Phase 6 (MVP-6a)
- `tests/lsp_fs_watcher_wakes.rs`: register lint over `**/*.rs`; spawn ingest + drain; write a `.rs` file via `std::fs::write` (no LSP did_change); assert `lsp_diags_by_uri` reflects the new diag within 1s. Requires Watchman on PATH; skip with `tracing::warn` if absent (CI default).
- `tests/lsp_fs_watcher_no_clobber.rs`: open buffer for `a.rs` (overlay text differs from disk); write disk; assert the next refresh's diag count matches the overlay (overlay wins), not the disk.
- `tests/lsp_fs_watcher_close_picks_up_disk.rs`: open buffer; edit disk; close buffer; assert the disk-content diags appear after close.

### Phase 7
- `tests/lsp_git_checkout_burst.rs`: simulate 1000-file `notify` burst; assert one expand call, p99 latency <1s.
- `tests/diag_publisher_dedup.rs`: publish same diag set twice; assert second `publish_diagnostics` is skipped via `last` hash.

## 11. Self-critique: perf at each step

### 11.1 `VfsOverlay::get` on every `SourceReader` read (hot path)
- **Worst-case input:** `ast_yaml` over 63k Rust files. `read_cursor_with_intern` called once per file per render.
- **Hot-path cost:** one `DashMap::get`. DashMap shards keyed by hash; lookup is ~50ns on a 50-entry map.
- **Memory:** ~64 bytes per entry + `Arc<str>` text (typically <100KB per buffer × 50 open files = ~5MB worst case).
- **Blow-up vector:** if someone pushes thousands of synthetic buffers via the RPC (no `did_close`), the map grows without bound.
- **Invariant that prevents it:** `lsp_buffer_close` is the only way to remove; LSP frontend tracks open buffers and emits `did_close` on disconnect via `tower_lsp` lifecycle. Add a hard cap of 4096 entries with LRU eviction (oldest version wins eviction). Cap protects against frontend bugs.

### 11.2 `MemoDepsCache::by_path` rebuild cost
- **Worst-case input:** 1M `_memo_deps` rows on daemon start (in-memory FactStore path only; daemon's `compact_sources.is_some()` path starts empty, see §4.2).
- **Hot-path cost:** one-shot at startup, derived from the `memo_deps_loaded` sqlite load loop. Each row = `dunce::canonicalize` (one stat syscall worst case, but the OS dentry cache makes a hot repo ~50ns) + `HashMap::entry().or_default().insert()` ~150ns. 1M × 200ns ≈ 200ms cold, plus ~50ms warm.
- **Memory:** ~80 bytes per `(OwnerKey, InKey)` pair (averaged path keying); 1M rows might collapse to ~10k distinct paths × 100 owners = 1M entries. ~80MB.
- **Blow-up vector:** runaway `_memo_deps` table growth from a buggy rule. Already bounded by the dirty-set flush at end of run (`flush_memo_deps` writes only changed pairs). PK-uniqueness in sqlite prevents row explosion within one (owner, in_key, source) triple.
- **Invariant:** `_memo_deps` size capped by the number of distinct `(owner, in_key, source)` triples a given rule set can produce; cardinality is bounded by rule count × ast match count per file × file count. For a 63k-file run, observed `_memo_deps` size ~63k × 2 = 126k rows. Two orders of magnitude below worst case.
- **Diff-replace cost:** every `record_memo_deps` reads the prior `forward.get(&key)` (one HashMap lookup), builds a `HashSet<&PathBuf>` of new paths (typical: 1-5 entries), iterates old paths once (same size), and prunes empty buckets. O(deps-per-input). Adds one HashMap lookup per write site (v2_ops.rs:873, 1830, 3064; mounted_query.rs:435, 519). At 63k rows that's 63k × ~50ns = 3ms over the whole run. Negligible.

### 11.3 `WakeEvent` dedup at 10k events/tick
- **Worst-case input:** `git checkout` of a 10k-file diff; `notify-debouncer-mini` collapses to ~100 batch events but each carries up to 100 paths.
- **Hot-path cost:** `on_wake_event(FsChange)` iterates the batch with two HashSet lookups per path (`paths_of_interest.contains`, `vfs.is_owned`). 10k * 2 * 100ns = 2ms.
- **`SourceIdentity` compute:** `git_oid` lookup on the per-toplevel `SourceIndex` is a DashMap point query (~100ns) if the index is already built. Cold build via `git ls-tree -r HEAD` is 200ms for 50k files (one fork+exec). Cached via `resolved_root_cache` (`v4/src/runtime_graph.rs:124`).
- **Mpsc send:** 10k owners × `try_send` = ~1ms.
- **Total:** ~5-10ms per burst.
- **Blow-up vector:** identity equality fails on every path (e.g., `git checkout` from a deeply diverged branch); every owner of every changed path wakes. If owner count per path is 100 (a hot util.rs imported everywhere), 10k paths × 100 owners = 1M jobs. Mpsc cap 1024 backpressures the watcher.
- **Invariant:** drain task dedups on `(owner, in_key)`; the `expand` call's warm-slice is unique-path. Cost downstream ∝ owner count × distinct in_keys, not file count.

### 11.4 `DiagPublisher` cross-URI batching
- **Worst-case input:** 5k diagnostics across 5k distinct .rs URIs in one expand result.
- **Hot-path cost:** 5k `publish_diagnostics` JSON-RPC notifications. Each is ~500 bytes serialized; tokio_lsp serializes serially through one stdout stream. ~1ms each = 5s total. Bad.
- **Mitigation:** batch into chunks of 100 URIs and pipeline through `tokio::join!` with a concurrency cap of 8 (write to stdout is serialized but JSON build is parallel). Net: 5k / 8 = 625ms.
- **`last` hash dedup:** skip URIs whose diag set hash matches the previous publish. Steady-state cost is near-zero after the first round.
- **Blow-up vector:** the lint flips between two states on every keystroke (e.g., `if cond { lint } else { no_lint }`). `last` hash never skips. Acceptable; the user IS toggling state.
- **Invariant:** `publish_diagnostics` is fire-and-forget; the daemon does not await acks. The slow point is JSON-RPC encoding, which is CPU-bound.

### 11.5 Watchman daemon dependency
- **Install requirement:** users need Watchman on PATH (`brew install watchman`, distro package, or `winget install Facebook.Watchman`). First connect attempt that errors emits a single LSP `window/showMessage` with the install command for their OS.
- **OS limits — solved by Watchman, not us:** Watchman handles per-platform watch primitives (FSEvents on macOS, inotify on Linux, ReadDirectoryChangesW on Windows). Linux `fs.inotify.max_user_watches`, macOS `kern.maxfilesperproc`, Windows handle limits are all Watchman's problem; the daemon is tuned for monorepo scale (Buck2, Mercurial, Sapling at Meta).
- **Daemon liveness:** `watchman_client::Connector::new().connect()` returns `Err` if the daemon is down. We auto-`watchman --foreground` ONLY if `--allow-watchman-spawn` flag is set; otherwise emit the install message and degrade to "polling on did_change only" mode (no FS reactivity).
- **Crash recovery:** Watchman daemon crash drops our subscription. `WatchmanIngress::drain_subscription` detects the connection drop, reconnects with exponential backoff (250ms, 500ms, 1s, 2s, max 30s), and resubscribes using the persisted `last_clock` from `_watchman_clock`. No event loss across daemon restart.
- **Cost at 500-repo scale:** one `watch-project` per root, one persistent socket connection (the `watchman_client` crate multiplexes subscriptions on one connection by default). Memory: ~64 bytes per `SubscriptionHandle` × 500 = 32 KB on our side. Watchman daemon RSS: typically <100 MB on a 50k-file Rust monorepo (per Watchman docs).
- **Network mounts:** Watchman degrades to polling on NFS/SMB; latency goes from <100ms to ~1s but it works. We get this for free; hand-rolled `notify` would silently miss every event.
- **WSL2 `/mnt/c`:** Watchman on WSL2 supports both the native ext4 case and `/mnt/c` (via polling on the Windows side); same automatic degradation.

### 11.6 Pre-warm walker
- **Worst-case input:** 1M-file monorepo, all `.rs`.
- **Hot-path cost:** `WalkBuilder` at 1µs/entry hot = 1s. Globset match per entry ~100ns = 100ms. Per-file mpsc send = 1µs hot = 1s. Total ~2.1s.
- **Memory:** WalkBuilder uses ~1MB; mpsc cap 1024 backpressures.
- **Blow-up:** mpsc cap 1024 means the walker blocks after 1024 unhandled events. With drain task processing ~1k jobs/s, walker stays bounded.
- **Invariant:** hard cap `max_files=50_000` + budget `5s` enforced inside the walker loop with `if walked > cap || elapsed > budget { return deadline_hit }`.

### 11.7 `did_change` debounce window
- Today's 80ms window (`v4/crates/sprefa-lsp/src/main.rs:26`) is per-URI. Verified at `:131-189`. No change.
- **Stress:** 100 keystrokes/sec on one buffer = 100 `did_change` events in 1s. Each waits 80ms, only the last one fires. Cost: 1 RPC + 1 expand per second, not 100.
- **Blow-up:** simultaneous keystrokes on N buffers = N expand calls/sec. Bounded by the user (no one types in 50 windows at once).

### 11.8 `Backend.docs` cache (existing)
- Per-URI `(text, version)`. One entry per `did_open`'d file. With wide-selector, can include any file the user has opened.
- Sized by # files opened in the session, ~10s typically. Each text is a `String` clone of the buffer. ~100KB × 50 = 5MB. Fine.

## 12. Open questions

1. Should we add a `SprfDiag.uri: Option<String>` field, or carry the cross-URI mapping in a parallel `HashMap<owner_uri, target_uri>` returned by `get_diags`? Field-on-DTO is simpler; map is more compact when most diags share a URI.
2. When the lint sets a span on a non-.sprf file but the file is also covered by another LSP server (rust-analyzer for .rs), should we annotate `source: "sprefa"` clearly so the user can tell which server raised which diag? (Today's `to_lsp_diag` already sets `source: "sprefa"` at `v4/crates/sprefa-lsp/src/main.rs:435`.)
3. Does `vscode-languageclient` v9 filter inbound `publishDiagnostics` by `documentSelector`, or does it accept any URI unconditionally? Verify by reading `node_modules/vscode-languageclient/lib/common/diagnostic.ts`.
4. How does Cursor's MCP bridge handle multi-server diagnostic merging when both sprefa-lsp and rust-analyzer publish on the same URI? Likely concatenated (both visible), but confirm.
5. Should pre-warm be feature-flagged off by default to avoid surprise CPU/IO on workspace open?
6. Is there a clean way to refresh `paths_of_interest` AND re-watch dirs WITHOUT dropping the existing `notify::Watcher`? `notify` lets you `unwatch`/`watch` on a live watcher; the cost is one syscall each.
7. For the burst-mode fallback (§11.5), what is the threshold for "switch to polling"? Default 50k watches is conservative; the actual ceiling depends on the user's `ulimit -n` and `max_user_watches`.
8. Should the daemon expose a `/lsp/pre-warm-status` HTTP endpoint so Claude Code can wait for `walked == queued && deadline_hit == false` before issuing its first `mcp__ide__getDiagnostics`?
9. The 80ms `did_change` debounce + 50ms `DiagPublisher` throttle add up. Is 130ms p50 fine for "agent sees diagnostic after typing", or do we need to overlap them?

## 13. Critical files

- `v4/crates/sprefa-lsp/src/main.rs` — Phases 1, 3 (extend initialize), 4 (RPC swap), 6 (publisher), 7 (throttle). Line ranges: `:26` (debounce), `:56-72` (Backend), `:131-189` (refresh), `:184` + `:416` (publish_diagnostics call sites), `:194-241` (initialize), `:396-417` (did_* handlers).
- `v4/src/app.rs` — all phases. `:89-156` (request types), `:470-485` (sprf_rpc!), `:491-522` (SprfState, DocState), `:779-792` (ingest writes), `:1451-1476` (existing handlers). Add `VfsOverlay`, `WakeEvent`, `Clocks`, `DiagSinkOut`, drain task, new RPCs.
- `v4/src/runtime_graph.rs` — Phase 5 (wrap field in `MemoDepsCache` struct + `by_path` + write-through + cold-load seed), Phase 6 (`Clocks::ephemeral` + path-keyed wake routes through existing `mark_memo_owner_dirty` at :994-1014, `dirty_memo_owners` at :1020, and `memo_dep_owners_for_source` at :965-984). Line ranges: `:100-145` (RuntimeGraph state, memo_deps_cache field is wrapped), `:610-661` (warm_changed_paths, warm_skip_predicate), `:684-832` (record_memo_dep / record_memo_deps / memo_deps_loaded / flush_memo_deps), `:965-984` + `:994-1014` + `:1020-1036` (existing dirty-sweep machinery, the wake-path Phase 6 plugs into).
- `v4/src/source.rs` — Phase 4 (`SourceReader` overlay seam, `Arc<VfsOverlay>` constructor arg).
- `v4/src/source_clock.rs` — Phase 6 (split `Clocks { durable, ephemeral }`; `FactStoreClock` stays + new `InMemClock`). Line ranges: `:67-117` (SourceClock trait + FactStoreClock).
- `v4/src/lsp.rs` — Phase 2 (extend `LspBodyComponent` and `resolve_diag_span` to record the target URI on the emitted `Diag`). Line ranges: `:141-250` (component), `:284-326` (LspBodyDef ctors).
- `v4/editors/vscode/src/extension.ts` and `v4/editors/vscode/package.json` — Phase 1 (selector + activation events). Line ranges: ext `:30-35`, pkg `:29-31`.
- `v4/Cargo.toml` — Phase 6 (add `watchman_client = "0.9"`). NO `notify`, NO `notify-debouncer-mini`. `globset = "0.4"` still wanted for the suffix-expression compile path.
- `.watchmanconfig` template — Phase 6 (ship at `v4/templates/_watchman_config.json`; `initialize` copies it to workspace root iff absent).

## 14. Companion plans

- `v4/plans/lsp-fs-watcher-reactive-wake.md` — the daemon-side reactive wake mechanism. §6 (FsWatcher + drain) of THIS plan is the diagnostic-publish consumer of that plan's `WakeEvent` ingress. Do not re-implement `VfsOverlay`, `WakeEvent`, `Clocks`, or `on_wake_event` here; extend exactly what that plan defines.
- `v4/plans/lsp-sprf-component-n-plus-1-lint.md` — the worked-test-case consumer. Phase 2 of this plan must produce diagnostics that route to the `.rs` fixture URI per that plan's expected output (3 diags on `n_plus_1_lint_target.rs`, 0 on the good fixture).
- `v4/plans/lsp-loop-justification-lint.md` — same shape; shares the justification-comment schema.
- `v4/plans/fs-streaming-peak-memory.md` — orthogonal; lands independently.

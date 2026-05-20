# Instance leak and memory-control plan — 2026-05-20

Targets the unbounded growth of `App.instances`, orphaned memo/source/file rows,
and the absence of an effect-system-level eviction protocol. Establishes the
invariants needed to reason about durability and memory ceiling together.

Audience: anyone touching `v4/src/app.rs`, `v4/src/dirty_source.rs`,
`v4/src/runtime_graph.rs`, `v4/src/store.rs`, or wiring a filesystem watcher.

Pre-reads:
- `chat_log/LATEST.md` for the conversation that prompted this plan.
- `plans/2026-05-19-clock-seam-invariants-plan.md` for the Generation/SourceId
  work this plan layers on top of.

---

## 0. Errata vs first draft (added 2026-05-20 post fact-check)

Three load-bearing errors in the first draft, corrected below:

1. **Leak path was misidentified.** First draft blamed `lsp_change` → `ingest`
   → `mount_pipe` push. Truth: `ingest` (`v4/src/app.rs:720`) builds instances
   locally per fused pipe (`fused.into_pipe().into_instance()` at line 734),
   expands them against a LOCAL `MemQueue` (line 731), no `MemoSeam` installed
   in `ExpandOpts` (line 732), and drops them at end of scope. The LSP path is
   purely diagnostic. The actual `mount_pipe` push site is `App::run` at
   `v4/src/app.rs:1905` — the `/run` RPC. Every `/run` invocation on the same
   path pushes a fresh `Arc<PipeInstance>` regardless of whether an equal one
   already exists. L1/L2/L6 below now target that path.

2. **`owner_op_id` format was wrong.** First draft said
   `sprf://ast/{ast_uri}/{source_uri}`. Truth: owner identity is
   `re_owner_hex(row, kind)` at `v4/src/v2_ops.rs:1694` =
   `blake3(pipe_hash ++ instance_id ++ depth ++ kind)`. It IS stable across
   re-runs because `inst.instance_id = identity` (`mount_pipe` at
   `v4/src/app.rs:643`) and `identity = stable_pipe_identity(path, pipe_ast)`
   (`v4/src/app.rs:2103`) is content-addressed. The `sprf://ast/...` strings
   in v2_ops.rs are subscribe URIs, not owner ids.

3. **Wake filter target was wrong.** First draft proposed JOINing
   `_memo_deps` with `_live_owners`. Truth: `dispatch_source_wake` at
   `v4/src/runtime_graph.rs:1476` traverses `runtime_source_subscription` and
   `runtime_source_subscription_compact` (declared at line 1816). The filter
   point is the graph traversal in `incoming_subscribers` (line 1680), not
   `_memo_deps`. Fix G below rewritten.

Mechanical drift also fixed: `Pipe` and `PipeInstance` live at
`v3/crates/effect_runtime/src/v2/expand.rs:26` and `:87` respectively, NOT in
a separate `pipe.rs`. `Pipe::into_instance` at `expand.rs:62`. `lsp_change` at
`app.rs:1457`, `lsp_close` at `app.rs:1461`. `SprfStore::intern_file` at
`v4/src/store.rs:336` (the `source.rs:139` mention is a call site).

---

## 1. Problem statement

Every file in v4 is an event source. Each `Pipe<Cursor>` lowered from a sprf
program is the re-entry point that gets re-run when its inputs change. The
`App.instances: Mutex<Vec<Arc<PipeInstance<Cursor>>>>` registry holds the
resident set used by `resume_mounted` (`v4/src/app.rs:650`) and
`drain_graph_jobs` (`:708`). It has no key, no dedup, no eviction.

Consequences today:

1. The `/run` RPC (`App::run`) calls `mount_pipe` once per fused pipe
   (`v4/src/app.rs:1905`) and pushes onto `self.instances`
   (`mount_pipe` body at `:646`). A second `/run` on the same path pushes a
   parallel `Arc<PipeInstance>` whose `instance_id` is the same content-derived
   identity but whose Arc is a separate object. `resume_mounted` then re-runs
   both. With `stable_pipe_identity` (`:2103`) the owner_op_id matches, so the
   seam Replays correctly — but RSS still climbs linearly in `/run` invocations.
2. There is no remove API. `lsp_close` does not touch instances (LSP did not
   add any), but anything mounted by `/run` is resident for the process
   lifetime.
3. `_memo`, `_memo_deps`, `_source_gen`, `_files` accumulate forever. No
   reference count, no liveness gate, no GC.

The LSP edit path is NOT a leak source today: `ingest` (`:720`) builds
instances locally, expands without a `MemoSeam` against a local `MemQueue`,
and drops them. No persistent memo or graph state is written from the LSP
flow.

The user goal is durable execution with as much memory control as possible.
Durable means the SQLite tier is the truth, restartable from cold. Memory
control means a daemon (or a long-lived TUI driving repeated `/run`) does not
grow without bound.

---

## 2. Leak taxonomy

| # | What grows | Trigger site | Today |
|---|---|---|---|
| L1 | `App.instances` Vec | `App::run` → `mount_pipe` push | `v4/src/app.rs:1905`, `:646`, no dedup |
| L2 | Resident pipes have no remove API | nothing calls eviction | n/a today |
| L3 | `_memo_deps` orphan rows | source removed, identity changed | no GC |
| L4 | `_memo` orphan rows | same | no GC |
| L5 | `_source_gen` orphan rows | watched file removed | no GC |
| L6 | Repeated `/run` duplicates Arcs | `mount_pipe` always pushes | `v4/src/app.rs:639-648` |
| L7 | `_files` orphan rows | content unreachable | no refcount |

L1 + L6 together are the daemon RSS leak. Same path + same content yields
the same `stable_pipe_identity` (verified at `v4/src/app.rs:2103`) and thus
the same `owner_op_id`, so the second push is functionally a duplicate. The
seam Replays correctly, but the Vec holds two `Arc<PipeInstance>` for what
should be one logical owner. Over N re-runs the Vec grows to N entries per
pipe per file.

The LSP edit path does NOT trigger L1/L6 (ingest is ephemeral, see §1).
L2 is a latent gap: even if we evict, today nothing calls it.

---

## 3. Invariants

These are the rules the design enforces. Each has a single owner site that
upholds it; everything else is allowed to assume it.

| Id | Property | Owner |
|---|---|---|
| I1 | Every `PipeInstance` has a unique `owner_op_id` key in the registry | `InstanceRegistry::upsert` |
| I2 | The registry maps `(source_path, pipe_identity) → owner_op_id → Arc<PipeInstance>` 1:1:1 | `mount_pipe` |
| I3 | `App::run` on path X with unchanged pipe content reuses the existing Arc instead of pushing | `mount_pipe` upsert path |
| I4 | Memo rows are trusted by the seam only when `owner_op_id ∈ _live_owners` | `MemoSeam` probe |
| I5 | `_source_gen` row exists iff at least one `_memo_deps` references it | `gc_sources` |
| I6 | `_files` row exists iff at least one `_memo_deps.content_id` or `_paths.file_id` references it | `gc_files` GROUP BY |
| I7 | A source wake re-runs only instances whose `owner_op_id ∈ _live_owners` AND that subscribe to that source | `dispatch_source_wake` filter via subscription tables |
| I8 | Re-running an instance with unchanged inputs writes zero `_memo` rows | seam Replay path (debug assert) |

I1–I3 are the registry rewrite (HashMap keyed by `owner_op_id` derived from
`stable_pipe_identity`). I4–I6 are the storage discipline. I7 keeps wakes
proportional to actual reachability. I8 is already true; the plan adds a
debug assertion so future seam edits cannot regress it.

Why key on `owner_op_id` instead of `(source_uri, ast_hash)`: the runtime
already uses `re_owner_hex(pipe_hash, instance_id, depth, kind)` as the SQL
key for `_memo_deps` and subscription tables (`v4/src/v2_ops.rs:1694`).
Reusing that key in the in-RAM registry means a single string identifies the
same owner across registry, memo, deps, and subscription. The per-op `depth`
and `kind` parts vary per op inside a pipe; the pipe-level identity used for
deduplication is the `pipe_hash == instance_id == stable_pipe_identity`
(`mount_pipe` at `v4/src/app.rs:642-643` sets both equal). So the registry
key is `u64` (the `stable_pipe_identity`), not a string.

---

## 4. Independence map

Eight invariants. Three independent rocks, the rest layered.

Independent:
- **A. InstanceKey + InstanceRegistry** (I1). Pure data-structure change.
  Replaces `Vec<Arc<PipeInstance>>` with `HashMap<InstanceKey, Arc<…>>` +
  source index.
- **B. `_live_owners` table** (I4 setup). Schema + insert/delete API on
  `RuntimeGraph`. Independent of A but together they fix L1/L2/L6.
- **C. `_files.refcount` column** (I6). Schema migration + counted
  intern/unload in `SprfStore`.

Layered on top:
- **D. lsp_close evicts instances** (I2). Depends on A.
- **E. ingest atomic swap** (I3). Depends on A.
- **F. Seam liveness gate** (I4 enforcement). Depends on B.
- **G. Dirty-source wake filter** (I7). Depends on B.
- **H. GC sweeps** (gc_owners, gc_sources, gc_files for L3/L4/L5). Depends on
  B and C.
- **I. Debug-only Replay write assertion** (I8). Independent, lands any time.

---

## 5. Ordering

```
phase-1  A   InstanceRegistry                 lands first, no behavior change yet
phase-1  C   _files.refcount column           SCHEMA_VERSION bump
phase-1  I   Replay write assertion           debug-only, no risk
phase-2  B   _live_owners table               schema + RuntimeGraph API
phase-2  D   lsp_close evicts instances       uses A
phase-2  E   ingest atomic swap               uses A
phase-3  F   Seam liveness gate               uses B
phase-3  G   Dirty-source wake filter         uses B
phase-4  H   GC sweeps                        uses B and C
phase-5  filesystem watcher operator          uses the whole stack
```

Phase 1 is mechanical. Phase 2 is the behavior change for L1/L2/L6. Phase 3
closes the wake-fanout hole. Phase 4 adds durability hygiene. Phase 5 ships
`watch_this_file()`.

Each phase compiles and passes gate on its own. No half-state.

---

## 6. Fix A — `InstanceRegistry`

Replaces `App.instances: Mutex<Vec<Arc<PipeInstance<Cursor>>>>` at
`v4/src/app.rs:508`. The existing identity scheme (`stable_pipe_identity →
u64 → inst.instance_id`) already gives us a key; the Vec is the only thing
that needs to change.

### Types

```rust
// v4/src/app.rs

pub struct InstanceRegistry {
    by_id:      Mutex<HashMap<u64, Arc<PipeInstance<Cursor>>>>, // key = stable_pipe_identity
    by_source:  Mutex<HashMap<PathBuf, HashSet<u64>>>,          // index for unload by path
    cap:        Option<usize>,
    lru:        Mutex<VecDeque<u64>>,
}

// Field rename: SprfState already has `pub registry: Arc<Registry>` at
// `v4/src/app.rs:520` (the operator/lower registry). The new field is named
// `instance_registry` to avoid collision. References in the plan have been
// updated.
```

`u64` key matches `inst.instance_id`, which is `inst.pipe_hash`, which is
the `identity` argument to `mount_pipe` (`v4/src/app.rs:642-643`). No new
hash function needed.

### Methods

```rust
impl InstanceRegistry {
    pub fn new(cap: Option<usize>) -> Self;

    /// Upsert. Returns prior Arc if displaced (caller drops outside lock).
    /// I1: identity is PK. Idempotent for same id → same Arc.
    pub fn upsert(&self, source: &Path, id: u64, v: Arc<PipeInstance<Cursor>>)
        -> Option<Arc<PipeInstance<Cursor>>>;

    /// Drop all instances mounted from a source path. Returns evicted Arcs.
    /// I2: caller is responsible for `retire_live_owners` after.
    pub fn unload_source(&self, source: &Path)
        -> Vec<Arc<PipeInstance<Cursor>>>;

    /// Cheap clone of all Arcs. For resume_mounted.
    pub fn snapshot(&self) -> Vec<Arc<PipeInstance<Cursor>>>;

    /// Snapshot the instances for a single source path.
    pub fn snapshot_source(&self, source: &Path)
        -> Vec<Arc<PipeInstance<Cursor>>>;

    /// Mark id as recently rendered for LRU.
    pub fn touch(&self, id: u64);
}
```

### Lock order

`by_id` then `by_source` then `lru`. Always the same order. Never hold two
across `expand`; `expand` runs outside locks via `snapshot` clones.

### `mount_pipe` rewrite

```rust
fn mount_pipe(&self, pipe: Pipe<Cursor>, source: &Path, identity: u64)
    -> Arc<PipeInstance<Cursor>>
{
    // Idempotent: if (source, identity) is already mounted, return the
    // existing Arc instead of pushing a duplicate.
    if let Some(existing) = self.instance_registry.lookup(source, identity) {
        return existing;
    }
    let mut inst = pipe.into_instance();
    inst.pipe_hash   = identity;
    inst.instance_id = identity;
    let inst = Arc::new(inst);
    self.runtime_graph.touch_live_owner_for_pipe(identity);
    let _ = self.instance_registry.upsert(source, identity, inst.clone());
    inst
}
```

The current signature is `mount_pipe(pipe, identity: u64)` at
`v4/src/app.rs:639`. Add the `source: &Path` parameter; the only call site
(`v4/src/app.rs:1905`) already has `&req.path` in scope.

### Component-side hashing

No new `Pipe::hash_shape_into` is needed because `stable_pipe_identity`
already hashes the `PipeAst` at the host layer
(`hash_pipe_ast` in `v4/src/app.rs:2107`). That hash is the registry key
and the seam owner key. The plan's earlier "add `hash_shape_into` to
`v3/.../v2/pipe.rs`" item is dropped.

### Files

- `v4/src/app.rs`: lines 503–522 (`SprfState` field), 508 (`instances`
  field replaced by `registry: InstanceRegistry`), 639–648 (`mount_pipe`),
  650–668 (`resume_mounted` reads `registry.snapshot()`), 708–718
  (`drain_graph_jobs` reads `registry.snapshot()`).
- No edits in `v3/crates/effect_runtime/`. `Pipe::into_instance` already
  exists at `v3/crates/effect_runtime/src/v2/expand.rs:62`.

### Risk

`resume_mounted`'s iteration order changes (`HashMap` is unordered). If any
test asserts on emission order across multiple pipes, switch to a multiset
compare. Run gate, then fix.

---

## 7. Fix C — `_files.refcount`

Schema bump in `SprfStore`. Adds a single column.

```sql
ALTER TABLE _files ADD COLUMN refcount INTEGER NOT NULL DEFAULT 0;
```

API on `SprfStore`:

```rust
impl SprfStore {
    /// Existing behavior plus refcount += 1.
    pub fn intern_file(&self, bytes: &[u8], path: &str) -> FileId;

    /// New. refcount -= 1; row not deleted (let gc_files do the sweep).
    pub fn release_file(&self, id: FileId);
}
```

Call sites for `release_file`:
- `unload_source` evicts the instance set; for each evicted instance, walk
  its `_memo_deps` and call `release_file` on each `content_id`. This is the
  one expensive part; batch the SELECT.
- alternative: skip the per-unload accounting and let `gc_files` infer
  refcounts from a single GROUP BY on `_memo_deps.content_id`. Cleaner; ship
  this.

Decision: ship `gc_files` as a single SQL pass, no per-unload counting.
Keeps the refcount column descriptive only, with the row count being the
authority during sweep.

Revised: do NOT add the column. The truth is a GROUP BY. Move this fix into
Fix H (GC sweeps). Saves a migration.

---

## 8. Fix I — Replay write assertion

`v4/src/memo_seam_impl.rs` Replay path. Add:

```rust
#[cfg(debug_assertions)]
{
    let writes_before = self.facts.write_count();
    debug_assert!(writes_before == writes_after,
        "Replay op wrote to _memo for owner {owner_op_id} in_key {in_key}");
}
```

`FactStore::write_count` is debug-only counter. If your `Arc<dyn
FactStore<Cursor>>` does not expose it, add a method behind
`#[cfg(debug_assertions)]`.

Reason: I8 today is a verbal property. Once registries and seam evolve, a
subtle bug in Replay can write rows that look correct but blow the no-write
guarantee, which downstream GC and replay debugging rely on.

---

## 9. Fix B — `_live_owners` table

Schema:

```sql
CREATE TABLE IF NOT EXISTS _live_owners (
    owner_op_id TEXT PRIMARY KEY,
    pipe_identity INTEGER NOT NULL  -- u64 stable_pipe_identity; index for retire_by_pipe
) WITHOUT ROWID;
```

API on `RuntimeGraph`:

```rust
impl RuntimeGraph {
    /// Insert all owner ids derived from a pipe identity. A pipe spawns
    /// many ops, each with its own `re_owner_hex(pipe_hash, instance_id,
    /// depth, kind)`. The runtime already records these ids whenever it
    /// writes `_memo_deps`; this helper duplicates them into the liveness
    /// table on mount. See "Liveness population" below.
    pub fn touch_live_owners_for_pipe(&self, pipe_identity: u64);

    pub fn retire_live_owners_for_pipe(&self, pipe_identity: u64); // DELETE WHERE pipe_identity = ?
    pub fn is_live_owner(&self, owner_op_id: &str) -> bool;
    pub fn live_owners_snapshot(&self) -> HashSet<String>;
}
```

### Liveness population

The plan does NOT precompute every per-op `owner_op_id` at mount time
(`depth` and `kind` vary per op, are only known during expand). Two options:

A. **Lazy insert.** Every `record_memo_dep` call also writes `_live_owners`
   if absent. Cheap (one extra `INSERT OR IGNORE`). Owner becomes live the
   first time it writes a dep. Until then it is effectively `Skip`-able.
B. **Track pipe identity, expand at filter time.** `_live_owners` keys on
   `pipe_identity`; the seam and wake filter join `_memo_deps.pipe_identity =
   _live_owners.pipe_identity`. Requires adding `pipe_identity` to
   `_memo_deps`.

Pick A. Reason: `_memo_deps` already exists with full schema
(`v4/src/runtime_graph.rs:648-655`), modifying it is more invasive than
adding two columns to a new side-table. Lazy insert also matches the actual
"owner becomes meaningful when it first emits" semantics.

### Retire semantics

When the registry evicts a `PipeInstance`, the runtime knows
`pipe_identity`. `retire_live_owners_for_pipe(id)` finds all per-op rows
that share `pipe_identity` via the indexed column and deletes them.

Crash safety: a crash between mount and the first `record_memo_dep` is
fine (owner stays Skip until first dep). A crash between
`retire_live_owners_for_pipe` and `registry.unload_source` would leave the
instance in RAM while owner rows are gone — `resume_mounted` would still
call `expand` on the orphan instance, but the seam would Skip every op.
Acceptable. Order at the call site: retire first, evict second.

### Owner-id format reminder

`owner_op_id = re_owner_hex(pipe_hash, instance_id, depth, kind)` per
`v4/src/v2_ops.rs:1694`. Because `pipe_hash == instance_id ==
stable_pipe_identity` (set in `mount_pipe`), all owner rows for one pipe
share the same `pipe_identity` value. That is the column we index.

Index: PK on `owner_op_id`. Additional index on
`(pipe_identity)` for retire-by-pipe sweeps.

---

## 10. Fix D — `unload_source(path)` API + LSP/CLI hooks

LSP doesn't currently mount anything persistent (see §1), so `lsp_close`
doesn't have instances to evict today. But the run-RPC path needs an
explicit unload API so a TUI / daemon / future watcher can drop a path's
instances.

```rust
impl SprfState {
    /// Drop all instances mounted from `path`, retire their owners,
    /// release any watcher registration. Returns count evicted.
    pub fn unload_source(&self, path: &Path) -> usize {
        let evicted = self.instance_registry.unload_source(path);
        let ids: Vec<u64> = evicted.iter().map(|i| i.instance_id).collect();
        for id in &ids { self.runtime_graph.retire_live_owners_for_pipe(*id); }
        drop(evicted);                                 // Arc → 0 outside locks
        ids.len()
    }
}
```

`lsp_close` (`v4/src/app.rs:1461`) keeps its current behavior (drop the
`docs[uri]` entry). It does NOT need to call `unload_source` today; revisit
when/if the LSP starts mounting persistently.

### Files

- `v4/src/app.rs`: new `unload_source` method on `SprfState` near
  `mount_pipe` (~640).
- `v4/src/bin/sprefa.rs`: new `unload <path>` subcommand for CLI parity.

### Risk

If two run-RPC calls used different paths to mount what hashed to the same
`pipe_identity` (unlikely; `stable_pipe_identity` mixes the path bytes),
unloading by path would not catch the other. Defensive: registry stores
`(path, identity)` and unload-by-path is exact. Verify the
`stable_pipe_identity` formula (`v4/src/app.rs:2103`) includes the path.
It does: `h.update(path.to_string_lossy().as_bytes())` at line 2105.

---

## 11. Fix E — Idempotent `mount_pipe` in `App::run`

The actual fix for L1/L6 is at `App::run` (`v4/src/app.rs:1905`), where
`mount_pipe` is called for each pipe in the program. Make `mount_pipe`
idempotent so a second `/run` on the same path with unchanged pipe content
returns the existing Arc instead of pushing a duplicate.

```rust
// v4/src/app.rs, replacing mount_pipe at :639
fn mount_pipe(&self, pipe: Pipe<Cursor>, source: &Path, identity: u64)
    -> Arc<PipeInstance<Cursor>>
{
    if let Some(existing) = self.instance_registry.lookup(source, identity) {
        return existing;          // L6 closed: no duplicate push
    }
    let mut inst = pipe.into_instance();
    inst.pipe_hash   = identity;
    inst.instance_id = identity;
    let inst = Arc::new(inst);
    self.instance_registry.upsert(source, identity, inst.clone());
    inst
}
```

At the call site (`:1905`), pass `&req.path`:

```rust
let inst = self.mount_pipe(pipe, &req.path, identity);
```

Re-run semantics:
- Same `(path, pipe_ast)` → same `stable_pipe_identity` (`:2103`) → registry
  hit → return existing Arc. No new instance, no new memo writes since the
  seam Replays.
- Same path, edited pipe content → different identity → new instance. The
  OLD instance still exists in the registry until `unload_source` is called
  or LRU eviction kicks in. Decision: rely on LRU cap + GC. Synchronous
  eviction-on-replacement would require knowing which instances belong to
  the old version of this program, which `App::run` does not track today.

Note: `ingest` (LSP path) does NOT call `mount_pipe` and is not modified.
Its locally-built instances are dropped at end of scope already (see §1).

### Files

- `v4/src/app.rs:639` (`mount_pipe` body)
- `v4/src/app.rs:1905` (call site, add `&req.path`)
- `v4/src/app.rs:508` (field replaced by `registry`)

---

## 12. Fix F — Seam liveness gate

`MemoSeam::probe` already checks `_memo_deps` content hashes for the
Replay/STALE decision. Add a precondition:

```rust
if !graph.is_live_owner(owner_op_id) {
    return SeamDecision::Skip;   // not Replay, not STALE: do nothing
}
```

A `Skip` decision means: do not run this op, do not emit, do not write. The
caller `resume_mounted` already iterates instances and tolerates per-op
zero-emit; verify.

This closes the window where a crash leaves an orphan `_memo_deps` row, the
process restarts, and the seam happily Replays into a registry that no
longer holds the owner.

### Files

- `v4/src/memo_seam_impl.rs` probe site
- `v4/src/runtime_graph.rs` `is_live_owner` helper

---

## 13. Fix G — Wake-fanout liveness filter — BLOCKED on identifier-space resolution

`dispatch_source_wake` at `v4/src/runtime_graph.rs:1476` builds the owner
set via `incoming_subscribers(source)` (`:1680`) plus
`compact_sources.owners_for_source(...)`. It does NOT query `_memo_deps`.

The intended filter point is `dispatch_wake` (`:1481`) after the BTreeMap of
owners is built. But the filter cannot be written as drafted because the
**wake graph and the memo deps live in different identifier spaces**:

| Surface | Key type | Source |
|---|---|---|
| `OwnerNode.uri_id()` in wake graph | `StringId` (intern of the **ast_uri string**, e.g. `sprf://ast/...`) | `declare_owner(ast_uri, ...)` in `v4/src/v2_ops.rs` ~`:2980`, then `graph.subscribe(&owner, ...)` |
| `_memo_deps.owner_op_id` | **hex blake3 digest** from `re_owner_hex(pipe_hash, instance_id, depth, kind)` | `v4/src/v2_ops.rs:1694` |
| `_live_owners.owner_op_id` (§9 draft) | same as memo_deps | proposed lazy insert at `record_memo_dep` |

A retain that matches `OwnerNode.uri_id().as_id_str()` (a decimal StringId
string, per `runtime_graph.rs:170-172`) against a `HashSet<String>` keyed
on blake3 hex will never produce a hit.

### Three resolutions to choose from before phase 3

**Option 1 — Key `_live_owners` by ast_uri instead.** Drop the
`re_owner_hex` indirection in `_live_owners`. Lazy insert in
`record_memo_dep` would need the ast_uri threaded through; today
`record_memo_dep` only sees `owner_op_id` as the already-computed hex.
Cost: thread `ast_uri` through the seam-record path. The seam's own
liveness probe would then need the inverse mapping (`re_owner_hex` →
`ast_uri`) which is not stored anywhere. Likely needs a new
`owner_op_id → ast_uri` table written at op-mount time.

**Option 2 — Key `_live_owners` by `pipe_identity` (u64).** Both the
subscription graph and `_memo_deps` can be associated with `pipe_identity`
because `mount_pipe` sets `pipe_hash = instance_id = identity`. Add
`pipe_identity` as a column to `_memo_deps` (or derive it: the existing
`owner_op_id` is `blake3(pipe_hash++instance_id++depth++kind)`; we cannot
invert that, so a new column is needed). Add a parallel column to the
`runtime_source_subscription_compact` table OR look it up via a
`subscriber_ast_uri → pipe_identity` side-table populated at
`declare_owner` time.

**Option 3 — Filter at the seam only, accept wake fanout cost.** Skip the
graph-level filter entirely. `MemoSeam::probe` already knows
`owner_op_id` (re_owner_hex) and can check `_live_owners` to decide Skip.
The wake will still mark retired owners' rows in `RUNTIME_DIRTY`, the
`resume_mounted` loop will still snapshot and `expand` orphan instances,
but every op will Skip. Wasteful, but correct. Best as a phase-3 stopgap
while Options 1/2 are evaluated.

### Decision required before phase 3

Pick Option 1, 2, or 3 before writing Fix G code. Recommend evaluating
Option 2 first (it is the natural shape if `_live_owners` is keyed on
something that survives multiple ops per pipe). Option 3 is the safe
shipping path for phase 3; phase 4 can promote to Option 1 or 2.

### Files (when unblocked)

- `v4/src/runtime_graph.rs:1481` (`dispatch_wake` filter point) — only if
  Option 1 or 2 chosen.
- `v4/src/memo_seam_impl.rs` probe site — under Option 3 this is the only
  edit.

This is the largest open question in the plan. Phase 3 is the natural
discussion point.

---

## 14. Fix H — GC sweeps

Three sweeps. All safe to run anytime since orphan rows are correct-but-
unreachable (Replay returns Skip on them after Fix F).

```rust
pub struct GcStats { pub deleted: usize, pub kept: usize }

impl RuntimeGraph {
    /// DELETE FROM _memo WHERE owner_op_id NOT IN (SELECT owner_op_id FROM _live_owners)
    /// DELETE FROM _memo_deps WHERE owner_op_id NOT IN (SELECT owner_op_id FROM _live_owners)
    pub fn gc_owners(&self) -> GcStats;

    /// DELETE FROM _source_gen WHERE source_id NOT IN (SELECT DISTINCT source_id FROM _memo_deps)
    pub fn gc_sources(&self) -> GcStats;
}

impl SprfStore {
    /// DELETE FROM _files WHERE id NOT IN (SELECT DISTINCT content_id FROM _memo_deps)
    ///                       AND id NOT IN (SELECT file_id FROM _paths)
    pub fn gc_files(&self) -> GcStats;
}
```

All three run in a single transaction inside `App::gc()`:

```rust
pub fn gc(&self) -> GcSummary {
    let owners  = self.runtime_graph.gc_owners();
    let sources = self.runtime_graph.gc_sources();
    let files   = self.sprf_store.gc_files();
    GcSummary { owners, sources, files }
}
```

Order is fixed: owners first, then sources (because gc_sources reads
_memo_deps which gc_owners just pruned), then files (reads _memo_deps too).

### Scheduling

Two modes:
- CLI: manual `sprefa gc` subcommand. No background thread.
- Daemon: timer thread calls `app.gc()` every `gc_every` (config field,
  default 60 s). Acquires no exclusive locks the seam holds; sqlite WAL
  handles concurrency.

### Files

- `v4/src/runtime_graph.rs`: `gc_owners`, `gc_sources`
- `v4/src/store.rs`: `gc_files`
- `v4/src/app.rs`: `gc()` plus `gc_every` config field
- `v4/src/bin/sprefa-daemon.rs`: spawn timer thread
- new `v4/src/bin/sprefa.rs` subcommand `gc`

---

## 15. Phase 5 — `watch_this_file()` operator

After phases 1–4 land, the watcher is a thin wrapper.

### Dependency

Add `notify = "6"` to `v4/Cargo.toml`. Verify against current notify
version policy: `rg '^notify' v4/Cargo.toml` returns nothing today.

### Types

```rust
pub struct WatcherHandle {
    inner: RecommendedWatcher,
    paths: Mutex<HashMap<PathBuf, usize>>,  // refcount per path
    cap:   usize,                            // watcher_path_cap
}

impl WatcherHandle {
    pub fn watch(&self, path: &Path) -> Result<(), WatchError>;   // refcount++
    pub fn unwatch(&self, path: &Path);                            // refcount--
}
```

### Op

New builtin in `v4/src/v2_ops.rs`. Surface: `watch_this_file()` returns Unit
and registers `path` (taken from the cursor's `at` coordinate) for
filesystem watch. On each emit, refcount++. On owner retire (via
`retire_live_owners`), refcount-- for each path the owner registered.

That last bit needs a side-table: `(owner_op_id → Vec<PathBuf>)`. Add to
`RuntimeGraph` as `_watched_paths(owner_op_id, path)` so the refcount
survives restarts.

### Notify callback

```rust
move |event: notify::Result<Event>| {
    if let Ok(ev) = event {
        for path in ev.paths {
            let source_uri: Arc<str> = Arc::from(path.to_string_lossy().as_ref());
            let generation = clock.bump_for_uri(source_uri.as_ref());
            runtime_graph.dispatch_source_wake(SourceWake {
                source_uri,
                generation,
            });
        }
    }
}
```

`SourceWake` field shape verified against `v4/src/runtime_graph.rs:337-340`:
`source_uri: Arc<str>` and `generation: u64`.

No shell. The filtered wake (Fix G) finds only live owners, runs only
reachable instances, and the seam does Replay or STALE per dep.

### Re-ingest from disk

If a watched file IS a sprf program (not just a data source), the watcher
must also re-ingest. Two options:

A. Op-level: `watch_this_file()` records whether the path is a sprf program
at register time. notify callback for sprf-program paths calls
`app.ingest(uri, fs::read_to_string(path)?, bumped_version)` before
dispatching the wake.

B. Caller-level: leave the watcher data-source-only. sprf programs are
handled by the LSP path. Daemon mode without an LSP runs an explicit "load
these sprf files" config and uses notify only for sprf-program files via
the same `ingest` path.

Decision: A. The op's whole purpose is to make any file a live event source
including sprf programs themselves.

### Files

- `v4/Cargo.toml`: `notify = "6"`
- `v4/src/app.rs`: `WatcherHandle` field on `SprfState`
- `v4/src/v2_ops.rs`: register `watch_this_file` builtin
- `v4/src/runtime_graph.rs`: `_watched_paths` table + API

---

## 16. Lifetimes table

| Type | Holds | Born | Dies |
|---|---|---|---|
| `InstanceRegistry` | resident pipe set | `SprfState::new` | process exit |
| `PipeInstance<Cursor>` | per-pipe state | `mount_pipe` | `unload_source` or `upsert` replace |
| `DocState` | host AST, diags | `lsp_open` | `lsp_close` |
| `_live_owners` row | liveness | `touch_live_owner` | `retire_live_owners` |
| `_memo_deps` row | input edge | first `record_memo_deps` | `gc_owners` after retire |
| `_memo` row | output cache | first commit | `gc_owners` after retire |
| `_source_gen` row | bump counter | first `bump` | `gc_sources` |
| `_files` row | source bytes | `intern_file` | `gc_files` when unreferenced |
| `WatcherHandle` | OS watch fds | `SprfState::new` (lazy) | process exit |

---

## 17. Storage layout deltas

Add:
- table `_live_owners(owner_op_id TEXT PRIMARY KEY) WITHOUT ROWID`
- table `_watched_paths(owner_op_id TEXT, path TEXT, PRIMARY KEY(owner_op_id, path)) WITHOUT ROWID`
- index `_memo_deps_owner_idx(owner_op_id)` if missing (gc_owners scan)
- index `_memo_deps_source_idx(source_id)` if missing (wake reverse-walk)
- `SCHEMA_VERSION` bump

Do not add `_files.refcount`. Use the GROUP BY on `_memo_deps.content_id ∪
_paths.file_id` (Fix H).

---

## 18. Sequence of reads/writes per operation

### `ingest(uri, text, version)` (LSP path)
1. host_parse, walk_program (no DB writes)
2. for each fused pipe: build local instance, `expand` with local `MemQueue`,
   no `MemoSeam`, drop at end of scope
3. docs[uri] = DocState
4. Unchanged by this plan; no persistent state touched

### `App::run(path)`
1. host_parse, walk_program
2. for each fused pipe:
   1. `identity = stable_pipe_identity(path, pipe_ast)`
   2. `mount_pipe(pipe, path, identity)` → upsert; reuses Arc if already mounted
3. `expand` each mounted instance with seam installed
4. `commit(generation, bus)`, `runtime_graph.flush(...)`, `sprf_store.flush()`
5. Live owners get written lazily by `record_memo_dep` calls during expand

### `lsp_close(uri)`
1. docs.remove(uri)
2. (no instances to evict from the LSP path)

### `unload_source(path)` (new API)
1. evicted = registry.unload_source(path)
2. for each evicted inst: retire_live_owners_for_pipe(inst.instance_id)
3. drop evicted Arcs outside locks

### file wake on `path`
1. clock.bump(SourceId::for_file(path)) → INSERT/UPDATE `_source_gen`
2. SELECT owners reachable AND live (Fix G join)
3. mark RUNTIME_DIRTY rows for those owners
4. resume_mounted snapshot → expand each instance with seam installed
5. seam probes `_live_owners` (Skip if missing), then `_memo_deps` content
   hashes (Replay or STALE)

### gc()
1. owners = DELETE memo/memo_deps rows for retired owners
2. sources = DELETE _source_gen rows with no deps
3. files = DELETE _files rows with no deps and no paths

---

## 19. Uniqueness conditions

- `InstanceRegistry.by_id`: `u64` (stable_pipe_identity) is PK
- `InstanceRegistry.by_source[path]`: set of `u64` ids mounted from that path
- `_live_owners.owner_op_id`: PK; `(pipe_identity)` indexed
- `_memo_deps`: uniqueness enforced procedurally by `record_memo_dep`
  (delete-by-`source_id` then insert per `(owner_op_id, in_key, source_id)`).
  No declared SQL PRIMARY KEY; treat as conceptual PK
  (`v4/src/runtime_graph.rs:648-691`).
- `_memo`: `(owner_op_id, in_key)` conceptual PK; verify against
  `v4/src/memo.rs` schema before phase 4
- `_files`: no declared SQL constraints. `FactStore::declare(FILES_TABLE,
  &["id", "content_hash", "path", "size"])` at `v4/src/store.rs:189` is a
  column-name-only metadata call. Uniqueness on `id` is enforced procedurally
  via `seen_files: DashSet<u64>` at `:100, :340`. `content_hash` is NOT
  enforced unique in SQL; `gc_files`' GROUP BY relies on the column being
  written deterministically per content, which the intern path guarantees
- `_watched_paths`: `(owner_op_id, path)` PK

---

## 20. Memory-control knobs

Defaults differ between CLI and daemon. CLI defaults to unbounded so
single-shot runs do not lose data to an unexpected eviction. Daemon defaults
to bounded so `RSS` is predictable.

| Field on `SprfState` (config) | Bounds | CLI default | Daemon default |
|---|---|---|---|
| `instance_cap: Option<usize>` | registry size | None | Some(2048) |
| `memo_row_cap: Option<usize>` | `_memo` rows; LRU on last_hit_tick | None | Some(1_000_000) |
| `gc_every: Duration` | scheduled sweep cadence | never | 60 s |
| `watcher_path_cap: usize` | notify path count | 1024 | 1024 |

`SprfConfig::load_default` (`v4/src/config.rs`) reads these. CLI sets via
flags. Daemon sets via `~/.config/sprefa/daemon.toml`.

---

## 21. Test plan

New unit tests:
- `instance_registry_upsert_idempotent_same_id`
- `instance_registry_unload_source_drops_all`
- `instance_registry_cap_evicts_lru_under_pressure`
- `run_twice_same_path_yields_one_instance_per_pipe` (drives L1/L6)
- `unload_source_drops_instances_and_retires_owners`
- `wake_skips_retired_owners` (drives Fix G)
- `gc_owners_keeps_live_drops_orphan`
- `gc_sources_drops_unreferenced`
- `gc_files_drops_unreferenced`
- `seam_replay_writes_zero_rows_debug_assert`

New integration test:
- `run_loop_n_times_bounds_instance_count_at_pipe_count` (drives L1/L6)
- `daemon_runs_for_10k_ticks_with_cap_holds_rss_bound` (asserts
  `instance_cap` is enforced)
- `lsp_open_change_close_does_not_grow_instances` (regression guard: LSP
  path must remain ephemeral)

Existing tests to audit:
- Anything that asserts on `resume_mounted` iteration order. Switch to
  multiset compare.
- Anything that asserts on `App.instances.len()` (currently a Vec; the
  registry's `len()` lives on `InstanceRegistry` after Fix A).

Gate: `cargo test -p v4 --release -- --nocapture` must stay GREEN
across each phase commit. (Package name is `v4`, per `v4/Cargo.toml:2`.)

---

## 22. Migration

Each phase ships a separate `SCHEMA_VERSION` only if it adds tables:

- Phase 1: no migration.
- Phase 2: `_live_owners` added. Migration is `CREATE TABLE IF NOT EXISTS`.
  No data backfill: existing memo rows are orphans by definition until the
  first re-ingest after upgrade, which re-populates `_live_owners`. The
  seam treats them as Skip until then (acceptable).
- Phase 4: indices on `_memo_deps`. `CREATE INDEX IF NOT EXISTS`. Safe.
- Phase 5: `_watched_paths` added. `CREATE TABLE IF NOT EXISTS`.

Old caches keep working with no manual rebuild needed.

---

## 23. Out-of-scope

- `_files` content compression. Separate plan if RSS proves bound by
  source-byte cache.
- Multi-process locking for concurrent daemons over the same sqlite. WAL
  handles read-during-write; concurrent writers are out of scope.
- LRU on `_memo` based on access frequency. Time-based GC + count cap is
  enough for v1.
- Cross-corpus owner sharing. `owner_op_id` is content-addressed but
  per-source; sharing is opt-in via future explicit `share { ... }` op.

---

## 24. Open questions

1. **GC cadence policy**. Synchronous on unload (lower memory hold, blocks
   the writer briefly) or scheduled (more durable, lets readers see
   stale-but-correct rows for a window). Plan picks scheduled. Confirm
   before phase 4.
2. **DROPPED.** `Pipe::hash_shape_into` is no longer required; the plan
   reuses `stable_pipe_identity` (`v4/src/app.rs:2103`) as the registry
   key. No `Component`-side hashing change is needed.
3. **owner_op_id stability across SCHEMA_VERSION bumps**. If the
   `re_owner_hex` formula changes (currently
   `blake3(pipe_hash++instance_id++depth++kind)` at `v4/src/v2_ops.rs:1694`),
   every memo dep row becomes orphan on upgrade. Acceptable since
   `gc_owners` reclaims, but flag in release notes.
4. **WatcherHandle in tests**. Tests should not actually register OS
   watches. Plan needs a `WatcherHandle::test_stub()` that no-ops `watch`
   but still tracks paths for assertion.
5. **Fix G identifier-space resolution (Option 1 vs 2 vs 3).** The largest
   open design question, fully discussed in §13. Must be decided before
   phase 3 begins. Recommend shipping phase 3 with Option 3 (seam-only
   liveness filter, accept wake fanout cost), promoting to Option 1 or 2
   in a follow-up.

---

## 25. Done criteria

- Every leak in section 2 has a closing PR linked here.
- A 10-minute keystroke-storm test on an LSP-mounted file produces a stable
  instance count equal to the pipe count of that file (L6 closed).
- A daemon-mode soak test of 24 h with a watched directory of 1000 files
  has bounded RSS and `_memo` row count under `memo_row_cap`.
- `App::gc()` is exercised by CI and reclaims rows after a
  `lsp_close` + sleep.
- All eight invariants in section 3 have a test that fails when the
  invariant is broken at the owner site.

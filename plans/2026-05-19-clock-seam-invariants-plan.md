# Clock/seam invariants fix plan — 2026-05-19

Targets audit items 3, 12, 13 plus related defects 4 (`String` gen parse), 5 (`SourceId` domain mixing), 6 (`GLOBAL_TICK`).

## 1. Independence map

Six defects. Three independent, three entangled.

Independent:
- **A. `SourceId` length-prefix.** `v4/src/source_clock.rs:31-36`. Pure hashing change.
- **B. `Generation` private field.** `v3/.../generation.rs:19`. Compiler flags external constructors; only `Generation(prev + 1)` inside `bump` and tests. Trivial.
- **C. `GLOBAL_TICK` per-`RtCtx`.** `v3/.../v2/expand.rs:192`. Carry through `ExpandOpts`.

Entangled:
- **D. `FactStoreClock::bump` RMW.** `v4/src/source_clock.rs:124-135`.
- **E. `RuntimeGraph::mark_dirty` RMW.** `v3/.../v2/runtime_graph.rs:642-660`.
- **F. MemoSeam `SourceIndex` keyed by git toplevel.** `v4/src/app.rs:1620-1632`, `v4/src/runtime_graph.rs:115`, `v4/src/memo_seam_impl.rs:325-337`.

D, E share transactional discipline. F is structurally independent but same-file diff. `String`-typed gen (item 4) is typing-only after B.

## 2. Ordering

```
phase-1  B (Generation newtype)      independent, lands first
phase-1  C (GLOBAL_TICK scoping)     parallel
phase-2  A (SourceId domain tag)     SCHEMA_VERSION bump
phase-2  D (FactStoreClock RMW)      depends on A in same migration
phase-3  E (RuntimeGraph RMW)        uses helper D adds
phase-3  F (per-root SourceIndex)    independent of D/E
phase-4  String→typed gen at read boundary
```

B is a compile-error wave. A forces regen, doing D in same migration keeps on-disk format stable across one transition. E reuses transactional helper from D. F is last because of multi-root behavioral surface.

## 3. Fix B — `Generation` newtype

```rust
// v3/crates/effect_runtime/src/generation.rs
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    pub const ZERO: Self = Self(0);
    pub(crate) fn next_after(self) -> Self { Self(self.0 + 1) }
    pub fn raw(self) -> u64 { self.0 }
}
```

`GenCounter::bump` keeps `AtomicU64::fetch_add(1, AcqRel)`, wraps with `Generation(prev + 1)`. Only `Generation(prev + 1)` at line 57 and tests use the ctor. Add `From<u64>` behind `#[cfg(test)]`.

Audit: grep `Generation(` in v3, v4 — only generation.rs internals + tests should compile.

Risk: zero. Files: `v3/.../generation.rs`. ~10 lines.

## 4. Fix C — per-`RtCtx` `ExpandTick`

Current: `static GLOBAL_TICK: AtomicU64 = AtomicU64::new(1)` in `expand.rs:192`.

```rust
pub struct ExpandClock { inner: Arc<AtomicU64> }
impl ExpandClock {
    pub fn new(start: u64) -> Self { Self { inner: Arc::new(AtomicU64::new(start)) } }
    pub fn bump(&self) -> ExpandTick { self.inner.fetch_add(1, SeqCst) + 1 }
}
```

One `ExpandClock` per `RtCtx`, mounted next to `gen: Arc<GenCounter>` in `RtCtx` (line 184–190 in `lib.rs`). `ExpandOpts::with_expand_clock(Arc<ExpandClock>)`.

Persistence: at `RtCtx` build time, read max(`expand_tick`) from `sqlite_queue` (column exists at `sqlite_queue.rs:67`) and seed. Fresh process restart no longer collides with stale rows.

`bump_global_tick()` becomes `opts.expand_clock.bump()`. `expand()` signature unchanged; `expand_tick` from `opts`.

Risk: a daemon hosting two corpora previously shared tick stream; afterwards each has its own. Desired property — `pull_runnable_batch_for(pipe_hash, instance_id, expand_tick, ...)` already gates by `(pipe_hash, instance_id)`.

Files: `v3/.../v2/expand.rs`, `lib.rs`, `v2/mod.rs`. ~60 lines.

## 5. Fix A — `SourceId` domain tag

Current: `h.update(b"src"); h.update(canonical_uri.as_bytes());`. No length prefix. `for_table("file:x")` and `for_file("x")` reduce to same input.

New:

```rust
fn hash_tagged(domain: &str, body: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&(domain.len() as u32).to_le_bytes());
    h.update(domain.as_bytes());
    h.update(&(body.len() as u32).to_le_bytes());
    h.update(body.as_bytes());
    *h.finalize().as_bytes()
}

impl SourceId {
    pub fn for_file(path: &str)   -> Self { Self(hash_tagged("file",   path)) }
    pub fn for_table(name: &str)  -> Self { Self(hash_tagged("table",  name)) }
    pub fn for_buffer(uri: &str)  -> Self { Self(hash_tagged("buffer", uri))  }
    pub fn of(uri: &str)          -> Self { Self(hash_tagged("uri",    uri))  }
}
```

Every existing `SourceId` hex digest in `_memo`, `_memo_deps`, `_source_gen` is now stale.

Migration:
- Bump `SPREFA_FACT_DB_SCHEMA_VERSION`. Stored in `_sprefa_meta(key, value)`. On open, if missing or older, truncate `_memo`, `_memo_deps`, `_source_gen`. Cold rebuild correct by construction.
- Single transaction in `RuntimeGraph::new`.

Risk: full cold rebuild on first run post-deploy. Document.

Files: `v4/src/source_clock.rs` (~30 lines), `v4/src/runtime_graph.rs` (~40 lines).

## 6. Fix D — `FactStoreClock::bump` transactional

Current shape: lock hot, read cur, drop lock, persist(delete+insert). Two threads observe N, both write N+1.

```rust
impl SourceClock for FactStoreClock {
    fn bump(&self, s: SourceId) -> Generation {
        let mut hot = self.hot.lock().unwrap();
        let cur = match hot.get(&s.0).copied() {
            Some(g) => g,
            None => self.cold_gen_locked(s),
        };
        let next = cur.checked_add(1).expect("gen overflow");
        self.persist_locked(s, next);
        hot.insert(s.0, next);
        Generation(next)
    }
}
```

Two readers can race on `current_gen` (Acquire load semantically). Writers cannot.

`persist_locked` becomes a real transaction. `SqliteFactStore` already exposes `with_connection<T>` at `fact_store.rs:889`; route delete-then-insert through `conn.transaction()`. `MemFactStore`: outer Mutex suffices.

Return type: `Generation` not `u64` (depends on B).

Stress test: spawn 64 threads × 1000 `bump(s)`; assert `current_gen == 64_000`. Both `MemFactStore` and `SqliteFactStore`. Add `v4/tests/clock_rmw_stress.rs`.

Files: `v4/src/source_clock.rs` (~50 lines), 1 new test.

## 7. Fix E — `RuntimeGraph::mark_dirty` transactional

Current: `read_where(RUNTIME_DIRTY, owner)` → iter().any() → if dup return → `insert_batch`. Two threads can both observe "no dup" then both insert.

Two routes:

(a) Declare `RUNTIME_DIRTY` with `UNIQUE(owner_id, source_id, generation)` and use `INSERT OR IGNORE`. Pre-check becomes hot-path optimization.

(b) Wrap select-then-insert in `conn.transaction()`.

**Pick (a).** Eliminates RMW window without holding lock.

Schema migration: `CREATE UNIQUE INDEX IF NOT EXISTS` on first open. Tied to same `SPREFA_FACT_DB_SCHEMA_VERSION` bump from §5 — one migration, one break.

Stress: 64 threads × 100 `mark_dirty(o, s, g)` identical. After barrier, assert `dirty_owners().len() == 1`. `v3/.../tests/dirty_rmw_stress.rs`.

Files: `v3/.../v2/runtime_graph.rs`, `fact_store.rs` + 1 test. ~40 lines.

## 8. Fix F — MemoSeam per-root keying

Bug: `v4/src/runtime_graph.rs:115` is process-wide `OnceLock<SourceIndex>`. First probe with `hint = "."` seals daemon CWD root for process lifetime.

Three options:

**(a) Seam-per-root keyed by absolute git toplevel.** Replace `OnceLock<SourceIndex>` with `DashMap<PathBuf, Arc<SourceIndex>>`. `source_index(hint)` resolves toplevel from `hint`, `entry().or_insert_with(...)`. Cost: one `git rev-parse` per never-before-seen root. Multi-corpus daemon gets multiple `SourceIndex` instances.

**(b) Lazy per-probe with cheap re-check.** Resolve toplevel on every probe. Per-probe `git rev-parse`. Reject.

**(c) Explicit init from corpus root with `Result<()>`.** `RuntimeGraph::init_source_index(corpus_root)` called once before any probe. Single-corpus only.

**Pick (a).**

```rust
struct RuntimeGraph {
    ...
    source_indices: DashMap<PathBuf, Arc<SourceIndex>>,
}

impl RuntimeGraph {
    pub fn source_index(&self, hint: &Path) -> Arc<SourceIndex> {
        let root = resolve_toplevel(hint)
            .unwrap_or_else(|| std::fs::canonicalize(hint).unwrap_or(hint.to_path_buf()));
        self.source_indices
            .entry(root.clone())
            .or_insert_with(|| Arc::new(SourceIndex::build(&root)))
            .clone()
    }
}
```

`resolve_toplevel` wraps `git -C hint rev-parse --show-toplevel`. Short-circuit: hint already a known root (walk up for `.git`).

Seam side (`memo_seam_impl.rs:325-337`): `hint = ...` block unchanged; `self.graph.source_index(&hint)` returns the right toplevel's `Arc<SourceIndex>`. Eager-seed at `app.rs:1632` becomes unnecessary.

The `.` fallback at `memo_seam_impl.rs:336` is dead under (a); when `deps_ch` is empty, `deps_of` already returned empty, seam already short-circuits `Miss`. Remove.

Files: `v4/src/runtime_graph.rs` (~80 lines: field + helper + 5 callers at 503, 561, 590, 630, 882), `v4/src/memo_seam_impl.rs` (~10 lines), `v4/src/app.rs` (delete 1620-1632).

## 9. Cleanup — typed `Generation` at runtime-graph boundary

`runtime_graph.rs:670-684, 826-840` parses `row.get("generation")` as `String` → `u64` via `.parse::<u64>().ok()?`. Malformed rows silently filtered.

Keep column TEXT on disk. At Rust boundary:

```rust
fn parse_generation(s: &str) -> Result<Generation, GenerationDecodeError> {
    s.parse::<u64>()
        .map(Generation::from_raw_unchecked)
        .map_err(|e| GenerationDecodeError::Malformed(s.into(), e))
}
```

`dirty_owners` and `continuation_for_owner` return `Result` (or push malformed into diag sink). `Generation::from_raw_unchecked` is `pub(crate)`.

Files: `v3/.../v2/runtime_graph.rs`. ~40 lines.

## 10. Migration discipline

| Change | Tables | Migration |
|---|---|---|
| §5 SourceId tag | `_memo`, `_memo_deps`, `_source_gen` | truncate on schema-version mismatch, cold rebuild |
| §7 UNIQUE index | `RUNTIME_DIRTY` | `CREATE UNIQUE INDEX IF NOT EXISTS` on open |
| §9 typed gen reader | none | n/a |

Single `SPREFA_FACT_DB_SCHEMA_VERSION` bump covers §5 + §7. Store in `_sprefa_meta(key TEXT PRIMARY KEY, value TEXT)`. On open: read schema_version, if missing or older, BEGIN IMMEDIATE → drop_table memo+memo_deps+source_gen → ensure UNIQUE on RUNTIME_DIRTY → upsert schema_version → COMMIT.

## 11. Test strategy

Multi-threaded stress:

- `v4/tests/clock_rmw_stress.rs` — 64×1000 bump, assert `current_gen == 64_000`. MemFactStore + SqliteFactStore.
- `v3/.../tests/dirty_rmw_stress.rs` — 64×100 mark_dirty identical key. After barrier, `dirty_owners().len() == 1`.
- `v4/tests/seam_multi_root.rs` — two tempdirs each `git init`ed. Probe alternating. Assert `source_indices.len() == 2`.
- `v4/tests/source_id_domain_separation.rs` — `for_file("table:imports") != for_table("imports")`. Length-prefix property.

Existing tests unchanged (clean DB) / exercise migration (stale DB).

No loom — invariants are sequential-consistency-trivial; bug is "drop lock between read and write". Stress with `criterion::bench_threads` plenty.

## 12. Risks during transition

**R1. Cold rebuild storm.** First run post-deploy invalidates every memo (§5). Document.
**R2. Mixed-binary DB.** Old binary + new binary share `--fact-db`. Old writes unprefixed; new truncates them; old truncates new on re-open. Mitigation: deploy `sprefa-run` and `sprefa-server` together; bump CLI major version.
**R3. F transition multi-root races.** Between landing `DashMap` and updating five callers, double-build risk. `entry().or_insert_with` discipline prevents this provided every caller goes through new helper. Make field private during patch.

Post-fix: only mutation is `bump`; type prevents `Generation(0)` elsewhere. Monotonic-per-process becomes structural.

## 13. Estimate

| Phase | Files | LOC |
|---|---|---|
| B | `generation.rs` | ~10 |
| C | `expand.rs`, `lib.rs`, `v2/mod.rs` | ~60 |
| A | `source_clock.rs`, `runtime_graph.rs` | ~70 |
| D | `source_clock.rs` + 1 test | ~70 |
| E | `runtime_graph.rs`, `fact_store.rs` + 1 test | ~60 |
| F | `runtime_graph.rs`, `memo_seam_impl.rs`, `app.rs` + 1 test | ~120 |
| §9 | `runtime_graph.rs` | ~40 |

Total: ~430 LOC across 9 files + 4 new test files. One on-disk schema break.

## 14. Verification checklist

- `cargo test -p v4 --release` clean against fresh `--fact-db`
- `cargo test -p v4 --release` clean against pre-populated `--fact-db` (migration path)
- Stress tests pass under `--test-threads=16`
- `grep -n 'Generation(' v3 v4` shows zero outside `generation.rs`
- `grep -n 'GLOBAL_TICK' v3 v4` shows zero
- `grep -n 'OnceLock<.*SourceIndex' v4` shows zero
- One-shot CLI on linux fixture: cold full, then edit one file → seam reports `Replay` for ~63k, `Stale` for 1

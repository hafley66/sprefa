//! Per-primitive N+1 stress tests.
//!
//! Today's 2026-05-20 corpus run hit two distinct N+1 bugs that the
//! existing `rule_invoke_batch_budget_target` test did NOT catch:
//!
//!   1. `mounted_query::reconcile_owner_table` → per-row
//!      `SupportLedger::add` → per-row `insert_batch(SUPPORT_TABLE,
//!      vec![1 row])` + per-row `triple_mult` full-table scan.
//!      Existing budget test passes because MemFactStore swallows the
//!      cost; only sqlite-backed corpus shows it as a 10-min wall.
//!
//!   2. `runtime_graph::source_index(hint)` → per-row
//!      `resolve_source_root(hint)` → fork+exec `git rev-parse
//!      --show-toplevel`. 63k file cursors × ~10ms/fork = ~10 min wall.
//!      Existing test never instantiates RuntimeGraph so it never
//!      surfaced.
//!
//! These tests hammer each high-call-count primitive in isolation,
//! assert side-effect counts AND a wall-time ceiling. The point is to
//! make each future regression FAIL in seconds, not 10 minutes.
//!
//! Shape, per primitive:
//!   - N >= 1000 calls.
//!   - Assert wall < 10s (CI ceiling) and ideally < 1s.
//!   - Assert side-effect counter delta is O(1) in N, not O(N).
//!   - Failure message names the bug shape.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use effect_runtime::v2::{FactStore, MemFactStore};
use tempfile::tempdir;
use v4::runtime_graph::RuntimeGraph;
use v4::source_index::SourceIndex;
use v4::store::SprfStore;
use v4::{Coord, Cursor};

/// Wall-time ceiling for every test in this file. Big enough to not
/// flake on a loaded CI box. Tight enough to catch the per-row
/// fork+exec or per-row insert_batch regressions that motivated this
/// file. Per-test asserts may impose a tighter ceiling.
const WALL_CEILING: Duration = Duration::from_secs(10);

fn assert_wall_under(label: &str, t0: Instant, ceiling: Duration) {
    let elapsed = t0.elapsed();
    assert!(
        elapsed < ceiling,
        "WALL-TIME REGRESSION [{label}]: took {:.3}s (budget {:.2}s). \
         If this fires, the primitive is back to per-call IO (fork+exec, \
         sqlite scan, or per-row insert_batch) regardless of how the \
         counters look.",
        elapsed.as_secs_f64(),
        ceiling.as_secs_f64(),
    );
}

fn sprf_store() -> Arc<SprfStore> {
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    SprfStore::new(facts)
}

// ── 1. SprfStore::intern_ref(coord) ──────────────────────────────────
//
// The contract: repeated calls with the SAME coord must be amortized
// O(1). The seen-set short-circuit means only the FIRST call actually
// calls `insert_core` (which buffers into `pending_core` and flushes
// to `insert_batch` once); the next N-1 calls return the cached Ref
// without touching the store.

#[test]
fn intern_ref_repeated_same_coord_is_o1() {
    const N: usize = 5_000;

    let store = sprf_store();
    let mem = store
        .inner()
        .as_any()
        .downcast_ref::<MemFactStore<Cursor>>()
        .expect("MemFactStore");
    let baseline = mem.stats().expect("stats").insert_batch_calls;

    let coord = Coord {
        repo: 7,
        rev: 11,
        fs: 13,
        lo: 17,
        hi: 29,
    };

    let t0 = Instant::now();
    let r0 = store.intern_ref(coord);
    for _ in 1..N {
        let r = store.intern_ref(coord);
        assert_eq!(r, r0, "intern_ref must be content-stable across calls");
    }
    assert_wall_under("intern_ref-same-coord", t0, Duration::from_secs(1));

    // The seen-set short-circuit means insert_core fires AT MOST once.
    // pending_core flushes on `inner()` (we already called it above to
    // get the baseline; the first intern_ref's row may sit in
    // pending_core until the next flush).
    store.flush();
    let after = mem.stats().expect("stats").insert_batch_calls;
    let delta = after - baseline;

    assert!(
        delta <= 1,
        "N+1 REGRESSION [intern_ref]: {} calls with the SAME coord \
         produced {} insert_batch calls (expected ≤ 1). The DashSet \
         seen-set short-circuit is broken; the hot LRU re-insert path \
         is hitting insert_core every call.",
        N,
        delta,
    );
}

#[test]
fn intern_ref_n_distinct_coords_is_one_flushed_batch() {
    // Companion: N DISTINCT coords should batch into ≤ 2 insert_batch
    // calls (CORE_FLUSH_ROWS=16384, N=5000 fits in one slot; one final
    // flush). Anything > 2 means insert_core is no longer buffering.
    const N: usize = 5_000;
    let store = sprf_store();
    let mem = store
        .inner()
        .as_any()
        .downcast_ref::<MemFactStore<Cursor>>()
        .expect("MemFactStore");
    let baseline = mem.stats().expect("stats").insert_batch_calls;

    let t0 = Instant::now();
    for i in 0..N as u32 {
        store.intern_ref(Coord {
            repo: 1,
            rev: 1,
            fs: i as u64,
            lo: i,
            hi: i.saturating_add(1),
        });
    }
    store.flush();
    assert_wall_under("intern_ref-distinct", t0, Duration::from_secs(2));

    let after = mem.stats().expect("stats").insert_batch_calls;
    let delta = after - baseline;
    assert!(
        delta <= 2,
        "BATCHING REGRESSION [intern_ref]: {} DISTINCT coords flushed \
         in {} insert_batch calls (budget ≤ 2). pending_core is no \
         longer collecting; rows are landing one-at-a-time.",
        N,
        delta,
    );
}

// ── 2. SprfStore::coord_of(ref) ──────────────────────────────────────
//
// The contract: a `coord_of` hit on the refs_lru returns immediately
// (no flush, no rows_of scan). A miss falls through to
// `inner.rows_of(WHERE_BYTES_TABLE)` which is an O(rows) linear scan.
//
// This test interns one coord, then calls `coord_of` N times. After
// the first call (LRU populated by `intern_ref`), zero growth in any
// FactStore counter is the budget.

#[test]
fn coord_of_repeated_hit_does_not_touch_store() {
    const N: usize = 5_000;
    let store = sprf_store();
    let mem = store
        .inner()
        .as_any()
        .downcast_ref::<MemFactStore<Cursor>>()
        .expect("MemFactStore");

    let coord = Coord {
        repo: 3,
        rev: 5,
        fs: 7,
        lo: 9,
        hi: 11,
    };
    let r = store.intern_ref(coord);
    store.flush();

    let baseline = mem.stats().expect("stats");

    let t0 = Instant::now();
    for _ in 0..N {
        let c = store.coord_of(r).expect("coord_of must hit");
        assert_eq!(c, coord);
    }
    assert_wall_under("coord_of-hit", t0, Duration::from_secs(1));

    let after = mem.stats().expect("stats");
    assert_eq!(
        after.insert_batch_calls, baseline.insert_batch_calls,
        "N+1 REGRESSION [coord_of]: {N} LRU hits grew insert_batch_calls \
         from {} to {}. coord_of is calling flush() on every hit.",
        baseline.insert_batch_calls, after.insert_batch_calls,
    );
    assert_eq!(
        after.insert_calls, baseline.insert_calls,
        "coord_of must not call insert on a hit"
    );
}

// ── 3. RuntimeGraph::source_index(hint) ──────────────────────────────
//
// The bug: even on a cache HIT the function called
// `resolve_source_root(hint)` to compute the dashmap key. That helper
// is a `git rev-parse --show-toplevel` fork+exec. 63k callers × ~10ms
// each = 10 minutes of pure fork-and-wait.
//
// The fix memoizes the resolution per-input-hint. After the fix a
// 1000-call hot loop completes in milliseconds. Without the fix the
// same loop runs for tens of seconds.

#[test]
fn source_index_repeated_same_hint_no_fork_loop() {
    const N: usize = 1_000;

    // Use the current cargo workspace as the hint; it is reliably a git
    // toplevel under our worktree layout, so resolve_source_root has
    // real work to do on the FIRST call.
    let hint = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new(store, facts.clone());

    // Prime the cache once — the FIRST call legitimately forks git +
    // builds the SourceIndex (which itself runs git ls-files / status).
    // We measure the HOT-LOOP wall, not the cold one.
    let _ = graph.source_index(&hint);

    let t0 = Instant::now();
    for _ in 0..N {
        let _ = graph.source_index(&hint);
    }
    let hot = t0.elapsed();
    assert_wall_under("source_index-hot-loop", t0, WALL_CEILING);

    // Tight ceiling: N memoized DashMap lookups should be sub-100ms on
    // any reasonable box. The actual hot path is two dashmap gets. If
    // this fires, `resolve_source_root` is being called on every hit
    // and the fork+exec is back.
    assert!(
        hot < Duration::from_millis(500),
        "FORK-PER-CALL REGRESSION [source_index]: {N} cache-hit calls \
         took {:.3}s (budget 0.5s). resolve_source_root(hint) is forking \
         `git rev-parse --show-toplevel` on every call instead of \
         memoizing the resolution.",
        hot.as_secs_f64(),
    );
}

// ── 4. SourceIndex::identity(abs) ────────────────────────────────────
//
// Contract: for a tracked-clean git path, `identity()` is two HashMap
// lookups (`dirty.contains`, `oid.get`) and a String clone. No `stat`,
// no `read`. The whole point of the SourceIndex is to make per-row
// identity O(1) HashMap probes after one batch `git ls-files`.

#[test]
fn source_index_identity_repeated_is_o1() {
    const N: usize = 5_000;

    // Build once over the workspace; the OID map is populated from
    // `git ls-files -s`. We pick a file we know is tracked-clean: this
    // very test file's parent directory.
    let hint = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let idx = SourceIndex::build(&hint);

    // Resolve the absolute path of a known-tracked file in the same
    // worktree. `Cargo.toml` always exists relative to CARGO_MANIFEST_DIR.
    let abs = hint.join("Cargo.toml");
    let _ = idx.identity(&abs);

    let t0 = Instant::now();
    for _ in 0..N {
        let _ = idx.identity(&abs);
    }
    let hot = t0.elapsed();
    assert_wall_under("source_index-identity", t0, WALL_CEILING);

    // 5000 HashMap hits + path strip + String clone should be well
    // under 200ms. A regression to per-call `stat` (e.g. dirty-set
    // miss) lands in seconds; per-call `read` + blake3 lands in
    // many seconds. Either triggers this.
    assert!(
        hot < Duration::from_millis(200),
        "IO-PER-CALL REGRESSION [SourceIndex::identity]: {N} calls on a \
         tracked-clean path took {:.3}s (budget 0.2s). The git/oid \
         fast-path is broken and identity() is falling through to \
         stat_identity or fs::read on every call.",
        hot.as_secs_f64(),
    );
}

// ── 5. Pre-par_render dispatch loop shape ────────────────────────────
//
// Reproduces the v2_ops.rs `AstNmComponent::dispatch` shape — the
// SERIAL loop over all rows that runs BEFORE `render_batch`. For each
// row it called `graph.source_index(self.root.as_path())`, hitting
// the resolve_source_root fork+exec we already test in #3.
//
// This test stresses N=1000 "rows" all pointing at the SAME hint,
// driving the same shape `dispatch` uses. A regression here means the
// pre-par_render loop is the bottleneck again.

#[test]
fn dispatch_pre_par_render_loop_shape_is_o1_per_row() {
    const N: usize = 1_000;

    let hint = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let abs = hint.join("Cargo.toml");

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new(store, facts.clone());

    // Cold prime: the first source_index legitimately runs git
    // ls-files / status. After this all subsequent calls should be
    // DashMap hits.
    let _ = graph.source_index(&hint);

    // Drive the dispatch-loop shape: per row, ask the graph for the
    // source_index, then ask the index for the path identity. That is
    // exactly what AstNmComponent::dispatch does in its serial
    // pre-par_render loop.
    let t0 = Instant::now();
    for _ in 0..N {
        let si = graph.source_index(&hint);
        let _id = si.identity(&abs);
    }
    let hot = t0.elapsed();
    assert_wall_under("dispatch-loop", t0, WALL_CEILING);

    assert!(
        hot < Duration::from_millis(500),
        "DISPATCH-LOOP REGRESSION: {N} rows of (source_index + identity) \
         took {:.3}s (budget 0.5s). The serial pre-par_render loop in \
         AstNmComponent::dispatch is back to per-row IO. Most likely \
         culprits: resolve_source_root fork-per-call, or identity() \
         falling through to stat/read.",
        hot.as_secs_f64(),
    );
}

// ── 6. Off-git hint (no fork at all) ─────────────────────────────────
//
// A defense-in-depth assertion: even when `hint` is NOT in a git tree
// (an empty tempdir), repeated source_index calls must not pay any IO
// per call. The first call may try `git rev-parse` once (and fail
// gracefully); subsequent calls must hit the memo.

#[test]
fn source_index_off_git_repeated_is_o1() {
    const N: usize = 1_000;
    let dir = tempdir().expect("tempdir");
    let hint = dir.path().to_path_buf();

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new(store, facts.clone());

    let _ = graph.source_index(&hint);

    let t0 = Instant::now();
    for _ in 0..N {
        let _ = graph.source_index(&hint);
    }
    let hot = t0.elapsed();
    assert_wall_under("source_index-off-git", t0, WALL_CEILING);
    assert!(
        hot < Duration::from_millis(500),
        "FORK-PER-CALL REGRESSION [off-git source_index]: {N} cache-hit \
         calls on an off-git hint took {:.3}s (budget 0.5s). Even the \
         off-git path is forking git per call.",
        hot.as_secs_f64(),
    );
}

---
created: 2026-08-16
updated: 2026-08-17
type: improvement
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
closed: 2026-08-17
closed_by: extract-driver
---

# leaf infra: content-keyed cache and parallel dispatch

## Description

## Description

The leaf's two declared infra pieces are unbuilt: the content-keyed extraction
cache and parallel dispatch. Every run re-parses every file, single-threaded,
and the crate description already claims otherwise.

## Receipts

| fact | receipt |
|---|---|
| the declaration | `v6/sprefa-extract/src/types.rs:1838-1839` ("LEAF INFRA (pure CPU; still this leaf): parallel dispatch (rayon, arena-per-worker); BlobSource impls + the (BlobHash, lang, mask) content-keyed cache") |
| cache is named pending on the key type itself | `v6/sprefa-extract/src/types.rs:50-53` |
| phase-2 key is specified but unbuilt | `v6/sprefa-extract/src/types.rs:1077-1081` (`(BlobHash, ProjectDigest, FamilyMask)`) |
| dispatch is one file, one thread | `v6/sprefa-extract/src/dispatch.rs:1-16` ("the generic rayon `dispatch` over many `ExtractJob`s + the arena-per-worker budget land in the parallelism lab (epic 4)") |
| rayon is not a dependency | `v6/sprefa-extract/Cargo.toml` `[dependencies]` has no rayon |
| the crate description claims it anyway | `v6/sprefa-extract/Cargo.toml` `description = "... Sync, rayon-parallel, arena-mastered ..."` |
| BlobSource impls DO exist (that half of the note is stale) | `src/project.rs:617` `SourceTreeBlobSource`, `:643` `FsBlobSource` |

## Fix shape

Two separable arcs; file the second separately if they are dispatched apart.

1. CACHE. `(BlobHash, lang, FamilyMask) -> ExtractOutput` for phase 1 and
   `(BlobHash, ProjectDigest, FamilyMask) -> Vec<ProjectEdge<F>>` for phase 2.
   Build-vs-buy applies before any bespoke map: the candidate set is `moka`,
   `quick_cache`, `lru`, and a plain `HashMap` behind the existing sync seam.
   Written candidate-by-candidate analysis first.
2. PARALLEL DISPATCH. rayon over the job list, one arena per worker. Budget-
   capped per the nothing-seizes-the-machine law.

Measure before and after on the scale corpus (`scripts/scale-invariants.sh`);
a cache with no measured hit rate is not landed.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
bash scripts/scale-invariants.sh
```

## Comments

### 2026-08-17T03:01:48Z · @extract-driver

HELD by extract-driver 2026-08-16: both halves cross the file-ownership boundary into the soopy driver's set, so no lane was dispatched. CACHE half: the key type is soopy::ContentId (crate hafley-rs, off-limits) and the seam it sits on is src/project.rs:617 SourceTreeBlobSource / :643 FsBlobSource. PARALLEL half: rayon needs a row in v6/sprefa-extract/Cargo.toml. Both files are the soopy driver's. RECEIPTS IN THE CARD BODY HAVE ROTTED since 331a2fa21 (BlobHash deleted, soopy::ContentId adopted crate-wide): types.rs:50-53 is now content_id_of/ZERO_CONTENT_ID, not the cache-key note; types.rs:1077-1081 is now ParseError, not the phase-2 key spec. Still true and re-verified at origin/main a4045153e: rayon is absent from Cargo.toml [dependencies] while the crate description at Cargo.toml:14 still claims 'Sync, rayon-parallel, arena-mastered'; src/dispatch.rs:1-16 still defers the rayon dispatch to the parallelism lab and dispatch() is one file, one thread; the LEAF INFRA declaration lives at src/types.rs:1838-1839. Re-cite before dispatching.

### 2026-08-17T03:02:03Z · @extract-driver

TRANSFERRED off extract-driver to the soopy driver by sprefa-coordinator word, 2026-08-16. Reason: both halves live in the soopy driver's file set (src/project.rs BlobSource seam, v6/sprefa-extract/Cargo.toml, and soopy::ContentId in hafley-rs). Card stays OPEN under the new owner; the receipt-rot warning in the note above still applies before dispatch.

### 2026-08-17T03:02:17Z · @soopy-driver

TRANSFERRED to soopy-driver from the extract driver (coordinator call): both halves live in soopy-driver files (v6/sprefa-extract/src/project.rs BlobSource seam + v6/sprefa-extract/Cargo.toml). Queued after the live soopy-full-wiring cards. NOTE: the card's receipts rotted at 331a2fa21 (BlobHash -> ContentId); every cited line needs re-verification before a lane brief is written.

### 2026-08-17T04:27:31Z · @soopy-driver

RECEIPTS RE-VERIFIED at origin/main 55e15e747 and the build-vs-buy analysis written: `plans/2026-08-17-extract-blob-cache-parallel.ANALYSIS.md`, PR #339. Corrections to this card's body: the LEAF INFRA declaration moved to `types.rs:2304-2305`; the phase-2 key spec to `types.rs:1445-1449`; the `types.rs:50-53` cache-key note is gone; the loops are `project.rs:456` (`read_inputs_plain`) and `:472` (`read_inputs_batched`); `BlobSource` impls at `project.rs:750`/`:765`. The extract-driver's "BlobHash deleted, ContentId adopted" note is now fully true: zero `BlobHash` hits under `v6/sprefa-extract/src`.

MEASURED, this machine, 12 logical cores. Whole-corpus IN-PROCESS pass (`extract --resolve` over all 2343 tracked source files): 4.67 / 4.32 / 4.38 s real, 4.00 s user, peak RSS 400 MB. The same corpus one-process-per-file: 24.3 s and 1084 MB of JSONL, so 20 of those seconds are process spawn and serialization, not parsing. Per-file wire output p50 306 KB / p90 1.24 MB / max 2.9 MB. Files sharing bytes with another file: 164 of 2346 (7.0%) in sprefa, 0 of 95 in hafley-rs.

THE FACT THAT DECIDES THE CACHE, and it is not in this card: extract runs IN-PROCESS inside the engine (`hosts.rs:46`, `:923`, the linked twin) and identical demands ALREADY fold within one batch (`hosts.rs:82`, `:1541`). The fold keys on the PATH, so a content-keyed cache adds exactly two things: the same path across TICKS, and two different paths with identical bytes. The intra-run win is 7% at best.

CACHE VERDICT: quick_cache 0.7 over moka 0.12 / lru 0.18 / dashmap 6.2 / plain HashMap, with clru, schnellru, foyer and cached also checked and rejected in the doc. quick_cache is the only candidate that is BOTH weight-bounded and concurrent without one global lock, and it costs ZERO new crates (its `equivalent` + `hashbrown` are already in both lockfiles). A weight bound is mandatory rather than a preference: 170 KB mean against a 2.9 MB worst case means an entry count does not bound memory.

PARALLEL half DISPATCHED as lane `feature-extract-parallel-dispatch` (flash4, base 55e15e747, brief `TASKS/extract-parallel-dispatch.BRIEF.md`): rayon, dedicated capped pool, order byte-identical, tests for order/skips/cap. rayon beats `std::thread::scope` plus manual chunking because the 10x per-file spread is exactly what work stealing exists for.

CACHE half NOT dispatched, two numbers need Chris: the memory cap (holding the whole corpus resident measured 400 MB), and whether `dispatch` may return `Arc<ExtractOutput>` rather than `ExtractOutput`, which is a public API change to the leaf at `dispatch.rs:14` plus both `project.rs` call sites, the bin, and the engine's linked twin.

### 2026-08-17T04:43:21Z · @soopy-driver

PARALLEL HALF LANDED, PR #341 merged at 924b8661fdd314bd0940bd9f9ddd2fba8b72cced. Both `read_inputs` loops are an order-preserving `par_iter` map on a DEDICATED rayon pool (never the global one), capped at `available_parallelism` clamped to 8 minus one held-back worker, overridable by `SPREFA_EXTRACT_THREADS`.

Measured on 2343 tracked source files, same binary shape before and after: 4.67/4.32/4.38 s real at 4.00 s user, to 2.53/1.84/1.83 s real at 4.60-4.77 s user. `user` unchanged and now above `real` is the receipt that the work is spread, not reduced. Peak RSS 400 MB to 441 MB. Gate `cargo test --features cli` 147/0 twice, `soopy-lockstep.py` PASS 127 twice; `Cargo.lock` gains exactly `rayon` and `rayon-core`, soopy's closure untouched.

Tests: order, skips and the cap rule are `#[cfg(test)]` units in `project.rs`, because input order is NOT observable from outside the crate (`sorted_lines` sorts the wire form and `read_inputs` is `pub(crate)`); `tests/26_parallel_dispatch.rs` asserts only what is observable, that the answer does not depend on worker count. Sabotages measured: reversing the flatten gives 4 passed 2 failed (the two order tests); dropping the held-back worker gives 5 passed 1 failed (the cap test).

Lane note for the record: dispatched as flash4 `feature-extract-parallel-dispatch`, which wrote the pool, both loops and the first cut of both test surfaces, then stalled on provider errors (68K context in 35 minutes, three supervisor re-prods). The driver finished it: dropped a dead `enumerate`, split `thread_cap_from` out of the env reader so the cap is testable without mutating environment from a threaded test binary, replaced a claimed SABOTAGE 2 that did not actually discriminate (output equality cannot see a thread count), and fixed a vacuous fixture whose files resolved to zero rows.

CACHE HALF still open. The coordinator answered the two numbers in auto mode (512 MiB default via `SPREFA_EXTRACT_BLOB_CACHE_MB`; `dispatch` may return `Arc<ExtractOutput>`), and the brief is written at `TASKS/extract-blob-cache.BRIEF.md`. It could not be dispatched alongside the parallel half: both own `project.rs`, `Cargo.toml` and `Cargo.lock`, so they are sequential, not disjoint. Note also that the call-site count is SIX, not five: `project.rs` (two), `bin/extract.rs:511`, `hosts.rs:1169`, `dep_resolve.rs:559`, `source_bind/_1_runtime.rs:389`.

## Resolution

### 2026-08-17T05:30:01Z · @extract-driver

CACHE HALF LANDED, PR #343 merged at origin/main 73fc90679; with PR #341 (parallel half) both halves are on main, card done. Shape: v6/sprefa-extract/src/cache.rs (quick_cache::sync::Cache with_weighter, key = (soopy::ContentId, Source::name(), FamilyMask folded to u8), weight = Strings::heap_bytes + nodes/edges * size_of per present family, cap 512 MiB default via SPREFA_EXTRACT_BLOB_CACHE_MB, one OnceLock instance); dispatch returns Option<Arc<ExtractOutput>> (src/dispatch.rs:19); the one types.rs edit is Strings::heap_bytes. Bin + three engine call sites compile unchanged through Deref; project.rs and three tests spell the Arc. tests/27_blob_cache.rs: counted hit (EXTRACTIONS AtomicUsize), two paths one blob, mask in key, eviction at 2x one entry weight via cache_with_capacity, cached-vs-direct wire identity. Grade by me on the branch merged with d1a5556b0: extract 156/0 twice (40 binaries), engine 92/0 twice, soopy-lockstep PASS 127, fmt clean. Timing extract --resolve over 3509 files: main 2.91/3.03/2.88 s 450-462 MB, branch 2.99/1.98/1.96 s 460-473 MB, wire byte-identical (cmp, 16.4 MB). Cargo.lock gained 7 packages (quick_cache defaults pull parking_lot chain), the brief's exactly-one claim was wrong. capacity_mb_from_env has no test of its own.

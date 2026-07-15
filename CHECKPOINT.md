checkpoint: bounded reactive storage and call-owner delta

## Where this started

A 107-file query returned zero call rows after roughly 3.7 seconds of source work, spiked CPU, used excessive resident memory, and exposed that path reactivity still crossed broad extraction and relational refresh boundaries. Requirements: two workers by default, bounded queues, one meaningful source sweep, backend-neutral storage seams, compact surrogate keys, no custom mmap dependency, and reproducible release measurements that cannot silently widen into a full rebuild.

## Architecture now in the tree

- `Storage` is a DBAPI-shaped general seam; call persistence has a narrower logical `CallStore` seam.
- SQLite SQL, TEMP tables, transactions, and physical call table names live in `src/storage/call.rs`, outside extraction.
- Call provenance is owner-scoped: interned repo/rev/path IDs, raw sites, resolutions, definition buckets, edge support, and a completeness marker.
- Full refresh rebuilds public relations and the private delta baseline atomically.
- A supported one-file WORK body edit replaces one owner and reprojects only affected call rows.
- Module refresh runs before call resolution. SCIP, module movement, definition movement, ambiguity, multi-repo/rev, deletion, and unsupported languages fall back loudly.
- File preparation, staged deltas, ownership/generation types, pipeline seams, plans, structural type work, comment-node research, dl self-maps, and reproducibility harnesses are included.

## Flow map

```text
changed paths
  -> source/path reconcile
  -> file/content/rev refresh
  -> SCIP pre-extract refresh
  -> module delta + dependency classification
  -> generation-local parse bundle
  -> extraction family refresh_paths
       -> call owner delta when preflights permit
       -> attributed call-family fallback otherwise
  -> node/spine refresh
  -> derived propagation

call owner delta
  -> marker/repo/rev/dependency/definition/name preflight
  -> collect old affected keys
  -> replace owner raw sites and resolutions
  -> collect new affected keys
  -> recompute support from provenance
  -> reproject affected public relations
  -> advance owner/marker generation
```

## Evidence actually executed

Tests are rails, not proof.

- `cargo check --lib`: completed with two build jobs.
- Call storage: 6 executed, 6 passed in 0.04 seconds.
- Rust call extraction: 1 executed, 1 passed in 1.13 seconds.
- Path tick: 1 executed, 1 passed in 0.35 seconds; 1.10 seconds including compile/link.
- Release `reactivity_probe` built with two workers in 59.58 seconds.
- The bounded 10-file release probe failed fast in 0.57 seconds and produced no valid performance result. Diagnostic rerun found its edit target had already been mutated and lacked the expected `helper_a` call. Report no timing from it.

## Known blockers

1. Delta resolution proves only unique callee names, while full resolution also applies module/import aliases. Reject relevant aliases or persist equivalent resolution dependencies before trusting delta output.
2. Module mutation and call delta-or-fallback are ordered but do not share one transaction. A call failure can expose new module state with old call state.
3. SCIP/module dependency digest readers fail open on SQL errors. Return `Result` and force attributed fallback.
4. The marker extractor digest becomes stale after delta and is not validated.
5. Call commit and `extract:call:WORK` digest save have a retry-safe but visible commit window.
6. The general storage seam still exposes SQL-shaped operations and some dependency reads use raw SQLite views. Call storage is isolated; whole-engine backend replacement is unfinished.
7. No integration rail yet combines builtin call extraction with `tick_paths`.

## Resume order

1. Add a conservative alias gate and an incremental-versus-clean alias rail.
2. Make dependency digests return `Result` and fail closed.
3. Add a caller-owned transaction spanning module classification, call delta/fallback, marker, public projections, and extraction digest.
4. Add the real call body-edit path rail and compare every public call relation plus support invariants with a clean rebuild.
5. Repair probe fixture lifecycle; rerun only 10 files, two workers, fallback forbidden. Capture wall time, files parsed, semantic digest, peak RSS, CPU, SQLite pages, and WAL bytes.
6. Only after valid size 10 evidence, run 100 then 1000. Do not run the broad default interactively.
7. Continue family-by-family backend extraction above `Storage`; prioritize corpus/dependency reads and source provenance. Do not build custom mmap.
8. Format only in a separate deliberate cleanup commit; this checkpoint avoids broad formatter churn.

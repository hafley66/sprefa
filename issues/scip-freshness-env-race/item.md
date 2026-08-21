---
created: 2026-08-21
updated: 2026-08-21
type: bug
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

`v6/sprefa-extract/tests/scip_freshness.rs` has a process-wide environment race.
It turned `cargo test --release --features cli` red 2 times in 7 runs on
2026-08-21; the same run is green in isolation and green 4/4 on re-run, so it
reads as noise rather than as a defect.

## RCA

`ENVIRONMENT` is a `Mutex<()>` at `tests/scip_freshness.rs:25`. Exactly two
tests take it:

| test | line | takes the lock |
|---|---|---|
| `explicit_index_override_ignores_the_set` | `:122` | yes, `:123` |
| `slow_indexer_is_a_named_skip_not_a_wait` | `:141` | yes, `:142` |
| `stale_set_rebuilds_and_the_original_set_still_hits` | `:53` | **no** |
| `a_stale_index_makes_ensure_rebuild_rather_than_reuse` | `:99` | **no** |
| `a_nested_checkout_is_never_staged` | `:191` | **no** |
| `a_persistent_stage_drops_a_source_the_corpus_deleted` | `:225` | **no** |

`explicit_index_override_ignores_the_set` sets `SPREFA_SCIP_INDEX`
PROCESS-WIDE at `:130` and restores it at `:133-136`. Rust runs a test binary's
tests on threads of one process, so during that window every sibling thread sees
the variable.

`index_path_for_set` reads it first, before any set logic:

```
v6/sprefa-extract/src/scip_ensure.rs:681   ///   1. `$SPREFA_SCIP_INDEX` when it names a file;
v6/sprefa-extract/src/scip_ensure.rs:695   if let Some(explicit) = std::env::var_os("SPREFA_SCIP_INDEX") {
```

So a sibling that runs inside that window is handed `elsewhere.scip` instead of
following its own set logic, and its assertion about reuse or rebuild is about
the wrong file.

The observed failure is `a_stale_index_makes_ensure_rebuild_rather_than_reuse`,
whose `assert!(!rebuilt.reused)` at `:111` is the one the override flips.

## Why it surfaced now

Nothing changed in this file. Adding `tests/33_v5_parity_matrix.rs` (this lane)
put one more test binary on the machine, which changed the scheduling and made
the window land differently. The race predates both.

## Fix shape

One line per test, the same line the other two already carry:

```diff
 #[test]
 fn a_stale_index_makes_ensure_rebuild_rather_than_reuse() {
+    let _held = ENVIRONMENT.lock().expect("environment lock");
     let root = temp_root("ensure");
```

and the same in `stale_set_rebuilds_and_the_original_set_still_hits`,
`a_nested_checkout_is_never_staged` and
`a_persistent_stage_drops_a_source_the_corpus_deleted`.

The sharper fix is that the override test should not mutate process state at
all: `index_path_for_set` could take the override as an argument the way
`ensure_index_for_set` takes a budget, and then no lock is needed anywhere.
That is the larger change and it is a design call.

## Fail-pre-fix test

Run the whole suite 10 times and count reds. Today it is roughly 2 in 7. After
the lock lines land it must be 0 in 10.

```bash
cd v6/sprefa-extract
for i in $(seq 10); do
  timeout 900 cargo test --release --features cli 2>&1 | grep -c '^test result: FAILED'
done
```

## Rail

`docs/failure-modes.md` entry owed on the fix, per the failure-ledger law.

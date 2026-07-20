# Test audit — 1,532 tests, five auditors, 2026-07-20

Triggered by three "green" results in one session that meant nothing (see
`.dl/test-false-green.dl` header for the incidents). Every `#[test]` in
`tests/it/**` and `src/**` was read by hand, bucketed, and quoted.

| bucket | count |
|---|---|
| OK | 1,484 |
| FALSE-GREEN | 22 |
| SKIPPED-PROBLEM | 2 |
| ENV-GATED | 15 |
| NOT-A-TEST | 8 |
| REDUNDANT | 1 |
| STALE | 0 |
| **total** | **1,532** |

## SKIPPED-PROBLEM — real defects parked behind an attribute (fix these)

1. `tests/it/retraction_props.rs:499` — **router footprint bug leaves public rels STALE.**
   `#[ignore = "...blocked on router empty-input footprint bug (seed cc 0d80eca0...)"]`
   Characterized by the file's own comment at 465-482: when a call-family input
   relation (e.g. `_call_def`) is EMPTY on the tick that first populates the
   router memo, `Ctx::scan` records no per-row deps for it, so the family's memo
   rel-footprint omits it. Later inserts into `_call_def` never trigger a rerun
   of `call_def`/`call_name`/`call_def_rev`, and the public rel stays stale.
   This is a correctness defect in the engine, not a perf skip. The reproducing
   seed is persisted in `tests/proptest-regressions/retraction_props.txt`, which
   is scoped to the FILE not the test — so the non-ignored 20-case sibling at
   :487 shares the strategy and property fn and may already be replaying it.

2. `tests/it/daemon.rs:1038` — **Linux per-directory watch pruning unbuilt.**
   `#[ignore = "fails until Linux per-directory watch pruning lands; this test is its spec"]`
   Fails by design. Watch count scales with ignored-subtree size. A permanently
   red spec that never runs; either build the feature or track it explicitly.

## FALSE-GREEN — 22, all one dominant shape

An early exit on a missing external tool, in a test with NO `#[ignore]`, so it
looks executed and green. Every one reports PASS on a machine lacking the tool.

    let Some(ra) = find_ra() else { eprintln!("SKIP ..."); return; };
    if !root.exists() { eprintln!("skip: ..."); return; }
    Err(_) => { tracing::warn!(...); return; }

Sites: oracle_go.rs:44, oracle_ts.rs:49, oracle_rust.rs:155, oracle_python.rs:53,
oracle_kotlin.rs:94, oracle_kotlin_parity.rs:42, oracle_madge.rs:198,
scip_name.rs:80, seam_bench.rs:162, flow_go_dispatch.rs:67, flow_py_dispatch.rs:59,
flow_xlang_scip_real.rs:70, perf_stress.rs:75, perf_stress_c.rs:78, plus src/ ones.
`oracle_go.rs`/`oracle_ts.rs` module docs state this as INTENTIONAL ("prints and
returns rather than #[ignore], so it participates in the default run") — the
documented intent is itself the anti-pattern.

Fix per site: `#[ignore = "<the actual need>"]`, or fail loudly. Never a silent
`return` that counts as coverage.

### A shape the rail does NOT catch
`tests/it/const_value.rs:153` — `Err(_) => {}` with a comment. An EMPTY arm:
no `return` for arm 2 to see, and arm 1 counts the `Ok` branch's asserts as
covering the whole test. Catching this needs per-path assertion reachability,
not a statement match. Known gap, deliberately not papered over.

## Coverage hole, not a test bucket
`examples/gh-cache.dl` had ZERO coverage repo-wide. All four "hermetic" tests in
`tests/it/gh_cache.rs` hand-write an inline `.dl` string mimicking the shipped
file; it could be deleted and they stay green. Correct pattern already in-repo:
`tests/it/flow_ctor.rs:79` uses `include_str!("../../examples/flow-ctor.dl")`.
Being fixed with fixture-driven e2e (no network, no `gh`).

## Harness defects found alongside
- `verify.sh` ran `cargo test` without `--no-fail-fast`: one flaky `--lib` test
  aborted the run before the 978-test `it` binary, and the flake-check then
  declared the run clean and stamped the tree verified. FIXED.
- `src/activity.rs` `SLOT` global static races under the default harness; this
  is what "load-sensitive, flakes under contention" actually meant for
  `lifecycle_round_trips`. Root cause named by the src auditor.
- `src/propose/mod.rs:845` read `{CARGO_MANIFEST_DIR}/../index.scip`, a path
  that stopped existing at the v5 lift (2026-07-01). FIXED; index generated.

## Rails now standing
`.dl/test-false-green.dl` — 3 arms (asserts-nothing, silent-skip, ignore-parks-a-bug).
Written in `sg` (structural), NOT `match` (line regex). The first draft used
`match` and found ZERO because `Err(_) => ... return;` spans three lines; the
`sg` rewrite finds 39 and catches every FALSE-GREEN all five auditors named
independently. sg also reaches inside `#[cfg(test)]`, which `call_def` cannot.
NOT yet wired into verify.sh — it would gate red at 50 findings today.

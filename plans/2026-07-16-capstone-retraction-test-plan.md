# Test plan — reactive retraction pipeline (post-P5)

System under test: the sole-writer family render path.
`refresh_call_rels` → `FamilyRouter::react_deltas` → `reconcile` → RowDelta →
`flip_call_rels_via_router` render (cold `reload_rel` / warm `retract_rows` +
`insert_rows`, one tx) → `sweep_gone_call_inputs`.

## Core invariant (the oracle)

For ANY sequence of edits E1..En applied to a working tree:

    state(incremental engine after ticking E1..En)
    == state(fresh engine extracting the final tree)

for all 7 public call rels, compared as decoded row sets. Every tier below is
this invariant restated at a different altitude, plus the failure modes that
can silently break it (stale rows, lost retractions, partial commits, memo
divergence). A second standing invariant: the render is atomic — an observer
never sees a rel between retract and insert.

## Tier map

| Tier | Altitude | Oracle | Status |
|---|---|---|---|
| T0 | frozen baseline | golden TSV fixtures | landed (call_golden, 7 rels) |
| T1 | unit: diff + write primitives | hand-computed sets | GAP: reconcile 0 direct tests; retract_rows chunk bounds untested |
| T2 | component: router + render | fresh-derivation snapshot | partial (skip asserts landed; no failpoint on flip tx) |
| T3 | integration: edit scripts | fresh-engine equivalence | partial (1 hand-built delta test; no deletion-heavy scripts) |
| T4 | property: randomized scripts | fresh-engine equivalence | GAP: none |
| T5 | lifecycle: restart / WAL / fs | cross-process snapshot | partial (staged_delta WAL test landed; no flip-path restart test) |
| T6 | performance | tick counter + budgets | rails exist; no retract-vs-reload bench |

## T1 — unit (highest defect leverage per line)

`reconcile(&prev, rows)` — currently zero direct tests; it is the diff engine
every delta flows through.

| Case | Expect |
|---|---|
| prev == next | empty delta |
| prev empty (never derived) | all-inserts delta |
| next empty (everything gone) | all-retracts delta |
| disjoint sets | full retract + full insert |
| overlap | exact set difference both ways |
| duplicate rows in next | reconcile dedupes or documents multiplicity (pin whichever row_key does) |
| one column differs by type-affinity (Int 1 vs Text "1") | treated as different tuples (full-tuple identity) |
| NULL-bearing tuples | NULL == NULL for identity (row_key semantics, pin it) |

`retract_rows` chunking (P0 fix; the bug class was param-budget overflow):

| Case | Expect |
|---|---|
| 0 rows | no statement executed |
| 1 row | deleted |
| exactly budget rows | one chunk |
| budget + 1 | two chunks, both applied |
| 2×budget − 1, wide tuple (8 cols) | budget computed per-param not per-row |
| row present twice in table | full-tuple DELETE removes all copies (or pin the alternative) |
| retract of a row not present | no-op, no error, count 0 |

## T2 — component (router + render)

| Case | Expect |
|---|---|
| cold family, fresh DB | reload_rel path; rel == derivation |
| cold family, stale rows pre-seeded on disk from a "prior process" | stale rows GONE after flip (the INSERT OR IGNORE hazard; this is the cold-guard's whole reason to exist) |
| warm family, empty delta | zero write calls (spy/counter), rel untouched |
| warm family, insert-only / retract-only / mixed delta | rel == memo rows exactly |
| react_deltas contract | returns EVERY rerun family incl. empty deltas, registry order; unchanged-input families absent |
| failpoint: insert_rows errors mid-render (2nd family of 3) | rollback — ALL public rels at pre-flip state, memo not poisoned (next flip recovers); mirror staged_delta's every-consume-failpoint test |
| flip called inside an existing tx (is_autocommit false) | no nested BEGIN; outer tx owns commit |
| unrouted family name | error, not panic |

## T3 — integration (scripted edit sequences, real extraction)

Each script ticks the engine per step, then asserts the core invariant vs a
fresh engine on the final tree. Deletion-heavy on purpose — insertion paths
had years of coverage; retraction is the new surface.

| Script | Targets |
|---|---|
| add file → tick → delete file → tick | rows fully retracted; no orphan sites/edges |
| delete callee only | call_edge retracts, caller's call_site stays |
| rename fn (delete+add same tick) | no duplicate syms; edge retargets |
| edit that produces identical rels (whitespace) | rerun with empty deltas, zero writes |
| rev retirement (git checkout away) | sweep_gone_call_inputs clears all 6 owned tables for the gone rev, in FK order; flip fires only if rows moved; second sweep is a no-op (idempotent) |
| interleaved: rev A gone, rev B live sharing files | B's rows untouched |
| empty repo / repo with zero calls | all rels empty, no error |
| same-tick add+delete of one file | net no-op |

## T4 — property (the exhaustiveness lever)

One generator, one oracle, high case volume for free:

- Generator: random edit scripts over a small synthetic Rust tree (ops:
  add-file, delete-file, add-fn, delete-fn, retarget-call, rename-fn,
  no-op-touch), 5–15 steps, seeded/shrinkable (proptest).
- Property 1 (equivalence): core invariant after every step, not just the last.
- Property 2 (delta soundness): rows(memo) after step k == apply(delta_k,
  rows before) — the memo and the rel can never diverge.
- Property 3 (idempotence): ticking with an empty changed-set changes nothing.
- Budget: ~200 cases CI, ~10k nightly/local.

This tier is why the unit tables above can stay small: enumerated cases pin
semantics; the property covers the combinatorial interior.

## T5 — lifecycle / process

| Case | Expect |
|---|---|
| kill process after commit, restart, tick a no-change set | cold reload reproduces identical rels (memo loss is safe) |
| kill between owned-input write and flip (failpoint) | restart converges — no permanently stale public rel |
| WAL reader concurrent with flip tx | sees pre-flip or post-flip snapshot, never mid (extend staged_delta's WAL test to the flip path) |
| known ledger bug: equal-length edit within one fs-timestamp tick | document as known-sparse; do NOT write a flaky test against it — fix enumerate_with_hash first |

## T6 — performance (regressions, not absolutes)

| Case | Expect |
|---|---|
| 1-file edit in an N-file repo | writes proportional to delta, not N (assert via tick counter / write-call count, not wall clock) |
| retract path per-row loop tripwire | tick counter screams; keep the existing rail green |
| warm no-change tick | zero rel writes |
| slow-rule / tick-over-budget rails | stay green under the flip path |

## Exit criteria

- T1 tables implemented and green (reconcile + retract_rows).
- T2 failpoint + cold-stale-rows tests green (the two real hazards P5 added).
- T3 scripts green under both a fresh DB and a reopened DB.
- T4 property suite green at CI budget with stored regression seeds.
- Full suite stays at 0 failed; goldens byte-identical throughout.

## Non-goals

- SQLite internals (chunk SQL syntax, WAL mechanics) — trusted, covered by
  rusqlite/upstream.
- Extraction correctness per language — owned by the extraction op suites.
- Wall-clock benchmarks in CI — counter-based assertions only; clocks flake
  (see perf_woes contention flake, 2026-07-16).
- Re-testing staged_delta atomicity — its 7-test suite already pins that
  surface; T2/T5 reference its patterns rather than duplicate them.

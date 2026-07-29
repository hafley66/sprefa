# RUNTIME BRIDGE ARC (golden plan phase 1; the alpha-critical seam)

Goal: the graded engine becomes the running engine. Two halves:
(1) hosts phase 2 -- live sh execution + live interval binds in the
tsv2 runtime (the named phase-2 unsupporteds from hosts wiring turn
into real execution); (2) a served process whose engine IS the
emitted incremental runtime, closing the two-disjoint-runtimes fact
from the utility review.

## The fork (decide by reading, then proceed; record the decision)

Path A (wrap, the DEFAULT): a thin serve layer grows around the tsv2
runtime (new files under v6/tsv2/, e.g. serve/), reusing v6/dl's
NAMED seams by import where the class-34 law allows (SqlRunner-class
runner, tracing channels, HTTP shape); v6/dl stays untouched and
running as the sibling.
Path B (adopt): v6/dl's server swaps its lowerSql evaluator for the
emitted tsv2 runtime. Choose B over A only if reading shows it is
MATERIALLY smaller and keeps dl 96/96 green; if you choose B, the
summary leads with the evidence.
Read FIRST either way: v6/dl/src/{3_runtime,6_http,1_hosts,1_binds,
0_trace}.ts, v6/tsv2/runtime/{tickLoop,1_incremental,ticklog}.ts,
v6/tsv2/scripts/run-emitted.ts, plans/2026-07-29-hosts-extraction-
verdict.md RX rows, CLAUDE.md's single-subscribe + rxjs laws.

## Scope

1. LIVE INTERVAL BIND: bind_decl(interval, ...) programs spin a real
   rx interval when served (BindConfig.scheduler precedent from F3:
   injectable scheduler, prod default async; teardown on program
   swap via switchMap; TestScheduler tests, no wall-clock sleeps).
2. LIVE SH HOST: probe demand rows execute the sh template
   (subprocess), decode line-per-column into the response rel as an
   EDB arrival (the F7-hardened parse shape; Number.isFinite
   rejection naming rel+column). Witness-digest dedupe: same witness
   never runs twice (content_addressed ruling); in-flight dedupe per
   witness (groupBy+take(1) shape, RX-H1).
3. SERVE: one cold observable is the app, exactly ONE .subscribe at
   the entry (the ratchet law, baseline 1 -- if path A adds a second
   main, the ratchet script and law need the same treatment as
   v6/dl's: state how the ratchet is preserved in the summary).
   Program load/swap over HTTP; boundary deltas streamed (SSE or the
   existing shape); DL_PERF_LOG tracing channel wired (class-34: use
   the P0 tracing spine, never a parallel pipeline).
4. GRADING, the non-negotiable: the schedule-fed byte grading stays
   the referee. Live mode receipts:
   a. door-handwritten served: POST its arrival batches over HTTP,
      tick log byte-identical to the oracle's schedule-fed log.
   b. a host program (clock bind + one sh host, e.g. a trimmed
      ghcacher slice or a purpose-built .dl6): bind fires on a
      TestScheduler-controlled clock in tests; sh runs a hermetic
      local script (no network); response rows land; the
      DETERMINISTIC PREFIX of the tick log (the rows whose values
      are schedule-determined) matches an oracle run fed the same
      answers as schedule. State which columns are non-deterministic
      and excluded, never hand-wave.
   c. leak: 20 program-swap cycles on the new server, handle counts
      flat by resource type + bind timer count returns to baseline
      (reuse the leak-soak assertions; extend leak-soak.sh or add a
      sibling gated in green-all).
5. Endurance law: no boot replay of answered demand (the witness
   cache is durable in sqlite; on restart, cached witnesses do NOT
   refire -- the endurance-goal phase-1 wedge). SIGKILL mid-run +
   restart receipt: no duplicate effect execution for cached
   witnesses.

## Out of scope (name refusals, do not build)

Extraction hosts (phase 2 of the plan), watcher, CLI, decode/2
compiler lowering (the json bucket), multi-probe rules, v5 anything.

## Grades (all re-run by you; coordinator re-runs after)

conformance 135/0, sweep both modes 70/67/0 zero movement, TEXT_DOOR
70/70/0, roundtrip ALL PASS, plunit 70/70 + growth, tsv2 tests grow
(the live receipts), import gate (gen files list may grow -- keep the
gate honest, never widen it silently), tsgo clean, dl 96/96 (path A:
untouched; path B: still green), store 74/74, ratchet accounting
stated, leak receipt, endurance receipt.

## Laws

Worktree agent. FIRST ACTION `git merge --ff-only <base sha stated
at dispatch>`; STOP AND REPORT on failure or missing v6/. Commit per
logical step with git commit -n; do NOT merge. Async-becomes-rxjs
(Promises only below the driver seam); interface-bound functions in
the header types file; descriptive identifiers; no em dashes; banned
words provenance, substrate, load-bearing, regime; refCount not
"support". Final summary: the fork decision with evidence, per-scope
receipts (a/b/c pasted), ratchet statement, all grades, cracks named.

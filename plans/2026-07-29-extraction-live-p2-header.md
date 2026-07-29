# Phase 2 header: extraction live (planner contract, seeded pre-dispatch)

Status: HEADER ONLY, arc not dispatched. Dispatch gates: memory-soak arc merged
(tsv2 runtime ownership overlap), watcher buy research landed
(plans/2026-07-29-watcher-buy-research.md), prolog org arc merged (v6/prolog
ownership overlap for any compiler-side rows).

Source of truth: plans/2026-07-29-v6-alpha-golden-plan.md phase 2 (lines 44-67),
plans/2026-07-29-hosts-extraction-verdict.md (term inventory),
plans/2026-07-29-sqlite-udf-graft-verdict.md (sidecar/in-process receipts).

## Fixed points (not up for relitigating in the arc)

- EXTRACTOR IS FIXED (user 2026-07-29): the existing sprefa-extract
  scip/ast-grep binary as-is; phase 2 wires its OUTPUT through hosts. No
  extractor redesign, no new extraction tooling research.
- Watcher is BOUGHT and is a BIND (spine_residency: never kernel); library per
  the research verdict; adapter follows the BindDef/BindRunner shape
  (v6/tsv2/1_binds.ts, interval clock bind is the precedent).
- Extraction hosts take the HOST shape (fork verdict): EDB arrivals
  content-addressed on (file_digest, query_digest); 1 invocation across N
  rules; feeds edge rules as ordinary deltas.
- REV SHAPE (ruled): no null; TWO hosts `enumerate(glob)` (worktree, unmarked
  default) and `enumerate_at(rev, glob)` (pinned, marked); programs union them
  like enum variant rels. Rev-pinned probes cache forever; worktree freshness
  rides the watcher salt.

## The arc must implement

1. Host execution phase 2 in the tsv2 runtime: the hostPlans/bindPlans/
   queryPlans data emitted by hosts wiring p1 becomes LIVE execution beyond
   the sh + interval pair the runtime bridge already grades. Named phase-2
   unsupporteds in the compiler flip to executing paths one by one, each with
   an oracle-graded fixture.
2. enumerate / enumerate_at hosts (the file-set feed), worktree-default
   enumeration replacing push-only file rows.
3. Watcher bind: file-change rows into EDB rels; batch per tick (commits are
   per tick, the watcher adapter coalesces into tick-shaped deltas).
4. sprefa-extract output through an extraction host: demand (file_digest,
   query_digest) -> fact rows, content-addressed cache per the salt ruling.
5. In-process vs sidecar per the UDF-lab receipts: prefer the shape with a
   proven receipt; record the fork in ARCH.pl fork/5 either way.

## Grading contract

- Every new live path gets an oracle-graded fixture (tick-log byte identity
  where the oracle can express it; replay grading via dl6_oracle.pl where
  world-fed, following the runtime-bridge TOTAL-replay precedent).
- EXIT RECEIPT (golden plan): an sg-rail-class diag rail runs end to end on
  v6 with a REAL file edit triggering the retick.
- Endurance: kill -9 during an in-flight extraction; answered witness
  exactly-once semantics per goal-endurance; no boot replay of answered
  demand.
- Count tests on any per-file path (statements flat across corpus sizes;
  formerly-quadratic law).

## Named slots (ambiguities the arc may hit; do not decide silently)

- SLOT-P2-WATCHER-EVENT-SHAPE: what of the chosen library's event vocabulary
  (create/update/delete/rename) crosses the bind seam vs collapses to
  (path, digest) rows. Renames are the hazard (editor atomic saves).
- SLOT-P2-ENUMERATE-SCOPE: glob residency (extraction ambiguity A1, still
  open) - glob as host demand column is the landed lab answer; confirm it
  survives contact with real ignore semantics (node_modules scale).
- SLOT-P2-EXTRACT-GRANULARITY: one host invocation per file vs per
  (file, query) batch; the fork verdict says 1 invocation across N rules -
  the batching spelling at the demand rel is the open part.
- SLOT-P2-STALE-DEMAND: watcher fires mid-extraction; whether the in-flight
  effect's answer for the old digest commits (content-addressed = it is a
  cache row, never stale, per salt_minting ruling) - verify, don't assume.

## Ownership at dispatch (fill exact shas then)

Implementation agent worktree per dispatch law. Owns v6/tsv2 runtime/serve
additions + fixtures; compiler-side rows (registry/host_expand) only if a
named refusal must flip, coordinated against the org arc's landed state.

# V5-PORT + PERF-TRACING ARC HEADER (planner-seeded contract, lab protocol)

User directive 2026-07-27 late: "start achieving port of v5 with perf tracing
throughout so we can know how we are doing." This header is the coordinator
contract for the arc. Implementation agents work in worktrees against it; the
coordinator owns main-tree files.

The arc executes the stopping-point milestone (CLAUDE.md: express the real
programs) with instrumentation installed FIRST so every port step is graded by
numbers against the v5 receipts, never by feel.

## The v5 yardstick (receipts, .agents/memory/project_org_scale_bench.md)

| metric | v5 number | source |
|---|---|---|
| cold multi-repo scan | 42,739 files / 389 repos / 5.9s (~7,244 files/s) | grafana corpus, `~/orgs/grafana` |
| RAM | flat during scan (disk-backed thesis) | same run |
| cross-repo seam graph | 110 hubs over 800 repos in ~10s | xrepo-go.dl |
| ghcacher loop | 12/14 revs fetched, content-addressed effect ids | progressive-revs.dl (not in tree; gh-cache.dl:1-141 is the in-tree twin) |

v6 existing instrumentation surface (do not duplicate):
- `v6/sprefa-store/js/src/engine/measure.ts` — memcap (soft RSS peak) + benchgraph.
- `v6/sprefa-store/js/src/engine/engine.ts:143` — one ad-hoc hrtime elapsed.
- `v6/dl/scripts/ingest_corpus.mjs` — cold-ingest driver (M9-before harness).
- `v6/dl/scripts/dbstat_report.mjs` — storage report.

## Phase 0 — perf tracing spine (BLOCKED on SLOT-LIB research, in flight)

Library is bought, never built (standing law; Rust side is the `tracing`
crate). A build-vs-buy analysis over node:perf_hooks, diagnostics_channel,
OpenTelemetry JS, tinybench/mitata, pino, clinic/0x is running; its verdict
fills SLOT-LIB. No bespoke tracing line lands before that verdict is in this
repo.

Contract, library-agnostic:

- **Seams** (exactly three, all existing function boundaries):
  1. `SqlRunner` (`engine/sqlRunner.ts`, the single driver seam): per-statement
     wall time + statement kind + rel name when known; aggregated per tick.
  2. Host effects (`v6/dl/src/1_hosts.ts`): per-effect span (spawn -> settle),
     effect id, exit/status.
  3. Ingest (`v6/dl/src/4_ingest.ts`): per-file span (extract wall, fact-line
     count, diff size), plus run-level files/s.
- **Emission**: one JSON line per tick carrying `{tick, wall_ms, stmt_count,
  stmt_ms_total, stmt_ms_max, effects: [...], ingest: {...}, rss_kb}`. rss_kb
  comes from the existing memcap sampler, no second RSS reader.
- **SLOT-ENVELOPE**: the tick-log format (stopping-point item 9, the marble
  record) gets its own header; perf fields must nest as ONE field of that
  envelope when it lands. Until then the perf line is standalone JSONL under a
  `perf` key, shaped so the later envelope wraps it without renames.
- **Zero new rxjs**: instrumentation wraps at function seams inside existing
  bodies. If an implementer believes an operator/tap is required, that is a
  stop-and-ask (standing plan item 4), not a judgment call.
- **Overhead budget**: instrumented ingest_corpus run within 5% wall of
  uninstrumented on the same corpus, measured and reported, else the mechanism
  is wrong.
- **Types**: any new class/namespace declares its interface in the package
  header types.ts (`I` prefix); important functions interface-bound.
- **Validation**: store 89/89, dl 74/74, both typechecks, conformance 97/97
  untouched, ratchet stays 1, goal-endurance 3/3, plus the overhead receipt.

## Phase 1 — ghcacher expressed in v6 (dispatchable NOW, worktree agent)

Stopping-point program 1. Extraction-lab discipline: ZERO engine or grammar
changes; where the v6 surface cannot express the v5 behavior, that is a
recorded FINDING against the language, never a patch.

- Twin: `examples/gh-cache.dl` (v5, 141 lines): poll -> fetch -> cache ->
  change_log carry. Drivers/gotchas in project_org_scale_bench.md (jitter
  desync, content-addressed effect id = (head,kind,args), term-extract/@next
  split).
- Ruled context the expression must honor: salt_minting = content_addressed
  (fills are cache updates, no stale state); effect_abort = best-effort;
  spine_residency = stdlib rels + binds, never kernel.
- **SLOT-SWR**: the SWR spelling under content salts is OPEN (CLAUDE.md).
  The draft uses the most direct spelling the grammar allows and files the
  spelling question as a named finding if ambiguous.
- Deliverable: `v6/dl/fixtures/ghcacher.dl` + a findings .md (worktree-local
  until landing; distills to plans/, per lab protocol) + a graded expectation:
  which ticks fire which effects (the marble list), checked against a canned
  no-network host stub if the harness allows, else stated as the gap.
- Grading when Phase 0 lands: the ghcacher run emits per-tick perf lines;
  effect counts per tick are the receipt that content salts dedupe (the v5
  12x-retick failure mode must NOT reproduce).

## Phase 2 — sprefa-extract cold-ingest parity (after Phase 0)

Stopping-point programs 4/6. `ingest_corpus.mjs` (instrumented) over a real
corpus; report files/s + RSS peak vs the v5 row. **SLOT-CORPUS**: full grafana
corpus (42,739 files, external ~/orgs) vs a pinned subset for repeatable runs —
user taste; default = one mid-size grafana repo pinned by rev for the
regression number, full corpus for the headline run.

## Ambiguity slots (named, per lab protocol)

- SLOT-LIB: tracing library (research verdict pending).
- SLOT-ENVELOPE: tick-log envelope (own header, item 9 keystone, next to write).
- SLOT-SWR: SWR spelling under content salts.
- SLOT-CORPUS: bench corpus pinning.

## Sequencing

P1 runs now (expression work, no tracing dependency). P0 starts when SLOT-LIB
verdict lands and is distilled here. P2 after P0. Every green state commits.

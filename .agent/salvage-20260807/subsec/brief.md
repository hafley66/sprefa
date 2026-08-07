# Lab: grid_10000 under 1,000 ms in the real engine

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/subsec && git merge --ff-only 173d308c`. On failure STOP and report.

## The mission
The dl6 entrant (the REAL compiled engine, prolog -> TS+SQLite) runs grid_10000 at 1,341 ms fixpoint after this morning's landings. Get it under 1,000 ms. Think inside the box and outside it: any change to the runtime (v6/tsv2/runtime), the compiler's emitted SQL (v6/prolog/lower.pl, emit_ts.pl), schemas, pragmas, statement shapes, or the driver loop is in bounds INSIDE YOUR WORKTREE. You have the whole tree. Wild ideas welcome; measured wild ideas only.

## What subsecond actually means, so you aim at the right wall
The pure-SQLite floor for this exact closure with ZERO engine machinery is 992-1,068 ms (v6/labs/exec_shootout/sqlite_raw/REPORT.md: one table, rowid+unique index, OR IGNORE, rowid-range delta). Subsecond means the full reactive engine does LESS SQLite work than that minimal script. So the path is deleting statements and btree writes, and possibly restructuring what the engine considers mandatory on a cold build. The engine's extra work over the floor is the target list.

## Receipts: races already run and LOST — do not re-run these
All in v6/labs/exec_shootout/sqlite_raw/ (REPORT.md, REPORT-BATCH.md):
- statement dispatch/batching: 2,582 dispatches cost 4 ms total; fusing statements is a no-op. Cutting statement COUNT only matters if it cuts WORK.
- double-hop unrolling: 2.4x LOSS (join work doubles).
- packed single-int keys: tie at best (pure-insert control: packed 6,565 ms vs two-col 6,777 ms on 10M rows — key width is noise next to btree page work).
- ORDER BY sorted inserts: loss (sorter pass costs more than append locality buys).
- NOT EXISTS prefilter beside OR IGNORE on identical storage: 1.4x loss.
- pragma sweeps on :memory:: 5-6% spread, nothing there.

## Known remaining costs, measure first then attack
1. Run `cd v6/labs/exec_shootout/dl6 && DL6_BENCH_UNBATCH=1 bash bench.sh` FIRST and put the per-statement grid profile at the top of your report. That table is your target list. (Inputs regenerate via the harness if .bench is missing: `cargo build --release` in ../harness, then it auto-runs; see REPORT-BATCH.md setup note. bench.sh rewrites FACTS.md — restore it with git checkout after runs.)
2. Known target: the per-round `__new_<rel>` fill (`arrivalASql`/`arrivalBSql` in the emitted dredSql plan) exists ONLY to feed the carry signal (`noteFill` reads its rowsAffected) and the `_sequence` staging that the unobserved-rel skip already deletes for this bench. On a skipped rel, the commit statement's own rowsAffected may be able to feed the carry instead, deleting one keyed write per row per round. Check `maintainHeadInPlace` in v6/tsv2/runtime/1_incremental.ts and the batch result indices before believing this.
3. The assert-walk commit is `INSERT OR IGNORE` into the WITHOUT ROWID head per round plus the wave-table writes (`__ping_`/`__pong_`). Count btree touches per derived row in the current plan text (.compiled/reachability.ts after a bench run) and compare against sqlite_raw's 3.
4. Anything else the unbatched profile names. Follow the measurements, not this list.

## Rules of evidence (non-negotiable)
- Every claimed win: same-session before/after, best of 2, checksum `9d7239568960d6a8` and derived 1,069,200 printed both sides. A checksum mismatch kills the experiment, full stop.
- One change per measurement. A combined candidate comes last, after its parts are priced.
- Keep a race log in REPORT.md as you go: idea, hypothesis, result ms, verdict (TAKE/REJECT), one line why.
- chain_10000 and layered_10000 must not regress more than 5% under your final candidate: run them once at the end (DL6_BENCH_FULL=1).
- The FINAL candidate must pass: `cd v6/tsv2 && bash scripts/sweep.sh` byte-identical (RUN wrong=0, FINAL wrong=0), `pnpm exec tsgo --noEmit` 0 errors, `pnpm test` green, plunit from v6/prolog/compile green. Experiments along the way may break anything; the landing candidate may not. If your best result cannot pass the sweep, report it as a FINDING with the exact reason instead of forcing it.

## Worktree setup you will need
`pnpm install` in v6/tsv2 AND v6/sprefa-store/js; `cargo build --release` in v6/labs/exec_shootout/harness; for committing, the pre-commit rail needs `cargo build --release --features cli --bin extract` in v6/sprefa-extract (~30s). Never npm. Never push. No subagents.

## Deliverables
Commits on lab/grid-subsecond (small, one idea per commit where practical). REPORT.md at v6/labs/exec_shootout/dl6/REPORT-SUBSEC.md: the opening profile, the race log, the final grid number with its gate table, and the honest verdict line: "grid_10000 fixpoint: 1,341 -> <N> ms" with what it took. You are pass 1 of 2; a review pass re-derives your wins, so isolating controls beat enthusiasm.

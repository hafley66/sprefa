# P0-B: price the refCount tail alone (the phase-1 stop-gate)

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/p0b && git merge --ff-only 86528155`. On failure STOP and report. If reality deviates from this brief, STOP and report. You are pass 1 of 2; a review pass (P0-B-R) grades whether your measurement is free of the walk's cost, so keep the tail isolated and obvious.

## Context
plans/2026-08-07-plan-ir-offload-contract.md (in your tree) section 4.3 projects that after the fixpoint walk moves to a rust executor, the SQLite "tail" is what remains: fill `__new_<rel>` (a bare-rowid table), antijoin against the head, `INSERT OR IGNORE` into the WITHOUT ROWID head, plus staging. The contract's phase-1 gate needs chain_10000 <= 12,000 ms; if the tail ALONE costs more than that, phase 1 is dead before any IR is written. Your job: measure the tail alone, on real data, no walk.

IMPORTANT SCOPE CORRECTION vs the contract text: a sibling lane is landing the unobserved-rel skip, which deletes the `__delta_`/`__frontier_` copy statements when nobody reads the rel (the bench boots that way). So measure TWO tails and report both:
- tail A (post-skip, the real phase-1 number): fill `__new_` + antijoin + `INSERT OR IGNORE` head only.
- tail B (with the delta+frontier copies included), so the report shows what the skip is worth inside this measurement.

## Task
One new file, `v6/labs/exec_shootout/sqlite_raw/exp_tail.mjs`, reusing common.mjs (openDatabase, readEdges, loadEdges, folds). Never edit any existing file.

Method, exactly:
1. Derive the closure once with the winner (`variants.loop_range_rowid.derive`) to obtain the true 10M-row set in derivation order (its rowid order IS derivation order; the contract's `emit` section explains why order matters).
2. Build the tsv2-shaped tables fresh in the same db: `__new_r` (source INTEGER, target INTEGER, plain rowid table), head `r` (source, target, __refcount INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (source, target)) WITHOUT ROWID, and for tail B also `__delta_r` (_sign INTEGER, _sequence INTEGER, source, target) and `__frontier_r` (_phase INTEGER, _sequence INTEGER, source, target), all TEMP tables like the emitted engine's.
3. Tail A timing: (a) INSERT INTO __new_r SELECT source, target FROM reachable ORDER BY rowid; (b) INSERT OR IGNORE INTO r (source, target) SELECT source, target FROM __new_r. Time (a) and (b) separately and summed.
4. Tail B timing: tail A plus (c) INSERT INTO __delta_r SELECT 1, rowid-1, source, target FROM __new_r; (d) INSERT INTO __frontier_r SELECT 0, rowid-1, source, target FROM __new_r.
5. Antijoin variant: repeat tail A with (b') the emitted engine's real shape, `INSERT INTO __new_r ... SELECT n.* FROM staging n LEFT JOIN r h ON h.source=n.source AND h.target=n.target WHERE h.source IS NULL` — read the emitted module at `v6/tsv2/gen_emitted/` for the exact antijoin text of a recursive head and mirror it; state in the report which module you copied from.
6. Run all of it on all three cases (`grid_10000`, `chain_10000`, `layered_10000`). Inputs: if `../dl6/.bench/*.in` are missing, regenerate exactly per REPORT-BATCH.md's setup note (`harness --engines ref --scales 10000`, harness may need `cargo build --release` in `v6/labs/exec_shootout/harness`). Best of 2 for chain, single runs fine elsewhere.
7. Verify each run: head row count equals the banked derived count for the case (REPORT.md table); print MATCH/MISMATCH.

## Report
`v6/labs/exec_shootout/sqlite_raw/REPORT-TAIL.md`: a table (case x {tail A fill, tail A insert, tail A total, antijoin variant, tail B total}), the verdict line "phase-1 tail ceiling on chain_10000 = <N> ms vs the 12,000 ms gate", and one paragraph on which statement dominates. Every ms from a run you executed.

## Boundaries and style
- You own ONLY exp_tail.mjs and REPORT-TAIL.md. No commits (the coordinator lands your files; the pre-commit rail needs a cargo binary your worktree lacks). No pushes, no subagents, no npm/pnpm installs.
- Comments max 2 consecutive lines. Banned words: provenance, substrate, load-bearing, regime, support.
- If chain exceeds 130s in any leg, record DNF and move on.

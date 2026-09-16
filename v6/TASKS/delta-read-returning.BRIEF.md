# delta-read-returning (PROBE, one of two arms; the other lane runs the competing design)

Issue: `issuectl show decoded-delta-from-stored` (numbers and the trigger-probe pricing correction are in its notes: cheap statements cost ~13 us, per-row work dominates). Base: worktree at or past b37ca81cfd551e16314a1c2189e40ed3add445c0. Branch `probe/delta-read-returning`. Push and hail numbers; NO PR unless hailed to.

## Design to implement
Capture staged rows at write time by adding RETURNING to the staging INSERTs and threading the returned rows to the consumers that today issue a second SELECT (read_staged, publish, delta select-back). Pure SQL surface, no process-memory carry beyond the statement result. Probe-gate behind an env flag.

## Read first
`v6/sprefa-engine-rs/src/incremental.rs` (stage_events, read_staged, publish, EdgeSink), `src/write_verbs.rs`, `docs/failure-modes.md` 85-89, the decoded-delta-from-stored issue notes.

## Measure, identical for both probe lanes, three runs each, medians in the report
ghcache 14-tick fold AND wide_64, at your base sha and on your probe arm:
- fold statements (SEAM_TALLY), wall ms, tick-only SQLite us
- per-verb (us, calls) table via DL_TRACE_SUMMARY=1
- receipts: tests/fixtures/ghcache_ticklog_base.txt byte-identical; RUST-GRADE graded=445 byte-clean=341 on the probe arm; default door byte-identical when the probe flag is off
Report shape: one table, baseline vs probe, same row order, so the coordinator can diff the two lanes cell by cell.

## You own
`v6/sprefa-engine-rs/src/` probe-gated, new test files. Forbidden: `v6/prolog/**`, `v6/dl/**`, conformance fixtures, main. The other lane owns the competing branch; do not read or copy its tree.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "probe returning: <table>"`. Blocked: one line, stop.

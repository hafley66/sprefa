(Coordinator note: agent harness blocked its own REPORT.md write; summary relayed from its final message.)

# t3 watcher (opus lane): defect did NOT reproduce; no fix invented

- 11 reproductions incl. the two real programs from the receipts (self-map, dataflow-rail): every edit delivered. NodeWatchSource + node's fsPromises.watch generator read line-by-line: no stop condition, no lost-wakeup window.
- Landed: v6/tsv2/tests/watchRealSource.test.ts (the missing real-fs N-saves-one-file regression test; red under a take(1) sabotage of the reported shape, green stock). tsv2 128->129, zero regressions; leak-soak green. 2_binds.ts byte-identical to base.
- 3 hazards found (not the bug): maxQueue 2048 silent drop unconfigured; non-abort watch error kills the whole server via process.exit(1); revert-to-known-content is invisible at the effect plane (content-addressed) and mimics deafness if read off artifacts.
- TWO OPERATOR-LEVEL EXPLANATIONS for the original receipts: TSV2_WATCH_ROOT mismatch, or `bop run` without --ticks self-terminating after BOP_RUN_IDLE_MS (default 2000ms) idle -- first edit lands, process exits ~2s later, all later edits land on a dead listener.

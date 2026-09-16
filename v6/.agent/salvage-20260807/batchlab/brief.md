# Lab lane: five speed experiments on the SQLite derivation floor

FIRST ACTION: `cd /Users/chrishafley/projects/sprefa-lanes/batchlab && git log --oneline -1` and confirm you are on branch `lane/batch-experiments`. If the tree is missing, STOP and report. If reality deviates from this brief at any point, STOP and report; do not improvise.

## Context, all measured, none of it yours to re-litigate
`v6/labs/exec_shootout/sqlite_raw/` holds the pure-SQLite baseline for transitive closure. Its winner (`loop_range_rowid` in variants.mjs): one table `reachable(source INTEGER, target INTEGER)` + `UNIQUE INDEX (source,target)`, semi-naive loop, delta as a rowid range, `INSERT OR IGNORE`. Banked bests (bench.mjs, this machine): grid_10000 fixpoint 992-1068 ms, chain_10000 9,212-9,798 ms, layered_10000 9,559-10,797 ms. Checksums that every experiment MUST reproduce: grid `9d7239568960d6a8`, chain `df09b2f409f8b9a8`, layered `addcf85b5162b9da`. Read `sqlite_raw/REPORT.md` first. Known anchors: pure keyed insert of the chain closure runs 1.34M rows/s; bare unindexed append runs 10M rows/s; an undeduped frontier DNFs (REPORT.md "what failed" item 1), so dedup during the walk is non-negotiable. Inputs are already banked at `v6/labs/exec_shootout/dl6/.bench/{grid,chain,layered}_10000.in`.

## The question
How much of the gap between 9.2s (current floor) and 7.5s (pure keyed insert) plus the insert cost itself can batching or storage tricks recover, inside SQLite, single-threaded?

## The five experiments, exactly these
Write `exp_batch.mjs` beside the existing files, reusing `common.mjs` (readEdges/openDatabase/fold). Each experiment prints one JSON line: `{exp, case, fixpoint_ms, fold_ms, derived, checksum, rounds, statements}`. Never edit `variants.mjs`, `common.mjs`, `bench.mjs`, or `REPORT.md`.

E1 dispatch-cost bound, then statement fusion. First measure the ceiling of the whole idea: time 2,582 no-op prepared-statement `.run()` calls on this db (that bounds what ANY dispatch batching can save). Then fuse each round's statement into `db.exec()` multi-statement text where the loop currently issues separate calls. Report both numbers; if the bound is under 100 ms, say so plainly, that result is as valuable as a win.

E2 double-hop unroll. One round derives two hops: `INSERT OR IGNORE INTO reachable SELECT known.source, e2.target FROM reachable known JOIN edge e1 ON e1.source = known.target JOIN edge e2 ON e2.source = e1.target WHERE known.rowid BETWEEN ? AND ?` PLUS the single-hop statement for the same range (both needed, or odd-length paths are lost; think about the frontier range bookkeeping and document it in the report). Chain has 2,580 rounds today; this should halve them. Verify checksum matches EXACTLY; if the range bookkeeping cannot be made correct, report the failure mode, that is a finding.

E3 packed single-integer key. `reachable(pair INTEGER PRIMARY KEY)` where pair = source * 4294967296 + target (all node ids fit 32 bits; assert that on load). The PK btree IS the table (rowid alias), so ONE btree does storage + dedup, and keys compare as single integers. Hop: `INSERT OR IGNORE INTO reachable SELECT (known.pair / 4294967296) * 4294967296 + edge.target FROM reachable known JOIN edge ON edge.source = known.pair % 4294967296 WHERE known.rowid BETWEEN ? AND ?` — note rowid = pair here so the rowid-range delta trick needs rethinking (rowids are no longer insertion-ordered); use a separate small frontier table or a wave pair, and say which you chose and why in the report. edge stays two-column WITHOUT ROWID, or packed too, race both if time allows. Fold: unpack for the checksum so it matches the banked value.

E4 sorted insert order. Take the current winner unchanged, add `ORDER BY known.source, edge.target` to the hop's SELECT so index inserts arrive sorted per round. Also race the packed E3 shape with `ORDER BY 1`. Btree inserts in key order are right-edge appends; measure whether SQLite actually exploits it here.

E5 best-of combination. Whatever E1-E4 won, combined into one variant, raced on all three cases, best of 2 runs, beside the `loop_range_rowid` baseline rerun in the same session (never compare against the banked numbers alone, machines drift).

## Report
`REPORT-BATCH.md` in `sqlite_raw/`: a results table (every experiment x every case run, fixpoint ms, delta vs same-session baseline, checksum MATCH column), one paragraph per experiment on what the number means, and a final verdict line: the best chain_10000 fixpoint achieved and what it took. Every claimed ms must come from a run you executed; single runs are fine for losers, best-of-2 for anything within 15% of the baseline.

## Boundaries and style
- You own ONLY `v6/labs/exec_shootout/sqlite_raw/exp_batch.mjs` and `REPORT-BATCH.md`. No other file, no git commits, no pushes, no subagents.
- Node is run directly (`node exp_batch.mjs ...`); dependencies come from the existing `../dl6/node_modules` resolution the sibling files already use. Never run npm or pnpm install.
- Comments: max 2 consecutive lines, only constraints the code cannot show.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support.
- If chain_10000 runs exceed 130s, kill that variant and record DNF, same as REPORT.md did.

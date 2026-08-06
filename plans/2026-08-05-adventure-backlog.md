# Adventure backlog: found today, parked on purpose

Everything here was found while doing something else. Each row is real, each has a
receipt, none of them is the current focus. The current focus is: fix the recursive
tick, then get the dl6 row into the exec_shootout standings.

## TOC
- Parked with receipts
- Already written down elsewhere
- Rules for waking one

## Parked with receipts

| # | adventure | receipt | why parked |
|---|---|---|---|
| 1 | density-aware dedup layout in the rust emitter | bitmap layout runs chain@10k in 37ms against sharded hash sets' 146ms, and needs 125GB at 1M nodes, so the emitter must choose from a cardinality estimate (INSIGHTS.md 2026-08-05) | a compiler feature, not a bug |
| 2 | `ingest.ts` composite-key collision | `v6/sprefa-store/js/src/engine/ingest.ts:497-500` joins column values with `"\|"` into a `Set` key, so `["a\|b","c"]` and `["a","b\|c"]` collide and one row is silently dropped from the insert | production, and Chris said fix labs rather than prod |
| 3 | `node_identity_key` same shape | `ingest.ts:226` | all-numeric fields today, so collisions need a stretch |
| 4 | packed ordinal in v5 | `src/engine/source_prepare.rs:581,588`, `(file_ordinal << 32) \| span_ordinal` stored as INTEGER | b-tree ordering survives the packing; only per-field filtering pays |
| 5 | fixpoint budget design | `plans/2026-08-05-fixpoint-budget.md`, six steps, V8 and k8s prior art | steps 1 and 2 are the current focus; 3 through 6 wait |
| 6 | oracle tick semantics under drain | the swipl oracle reaches full closure in one tick (`bench-cli/adapters/oracle.sh` against `sched1.json`); a drain-based engine would emit the same rows across two ticks and the tick-log comparison would differ | decides whether the budget can ever spread rounds across ticks |
| 7 | mono still unprofiled at 1M nodes | rxgraph beats mono at chain@1M (23.5M against 19.4M rows/sec) where a million near-empty hash tables get allocated | one row of the standings, not the headline |
| 8 | worktree sweep | 21 worktrees, `git worktree remove` blocked by the permission classifier | needs Chris to run the removals |
| 9 | interp dedup set is now 30% of its profile | `sample` on the fixed interp, layered@100k | the remaining cost is the exhibit, so this is optional |
| 10 | `prose-prod` block-grading bug | `prose-prod.mjs:159-171` grades each mdast text node separately, so inline code splits a sentence | moot if the dl6 port replaces it |

## Already written down elsewhere

| topic | where |
|---|---|
| recursive tick defect, full chain | `docs/failure-modes.md` entry 41 |
| budget design and build order | `plans/2026-08-05-fixpoint-budget.md` |
| catalog next increment | three competing plans in `sprefa-lanes/cat{flash,opus,terra}` |
| dl6 benchmark lane and why it stopped | `sprefa-lanes/dl6shoot/STOP.md` |
| shootout findings, all of them | `v6/findings/INSIGHTS.md` section 2 |

## Rules for waking one

- A row leaves this file only when it becomes someone's focus, and it leaves with
  its receipt attached.
- Adventures get an opus subagent, never the main thread, and never during a
  focused arc.
- A row with no receipt does not belong here.

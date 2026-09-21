# The sqlite layers, in plain words

This is the plain-words twin of `2026-09-21-sqlite-era-review.md`. Same review.
Read only, no code changed. Every claim has a file and a line.

## What a "layer" means here

A layer is one place where sqlite holds rows that a program reads, plus the
thing that keeps those rows right when the inputs change.

## One row per layer

| era | layer | holds | kept right by | where the upkeep starts |
|---|---|---|---|---|
| v5 | derived tables | one table per derived relation | wipe a component, then refill it | `v5/src/engine/derive.rs:334` |
| v5 | call facts | one owner's call sites and definitions | replace one owner as a unit | `v5/src/storage/call.rs:248` |
| v5 | a test fixture | three small tables | scoped deletes, test only | `v5/src/engine/deltaflow.rs:116` |
| v6 | tsv2 runtime | level tables, delta tables, frontier tables, cone tables | counts and signed deltas, plus a rederive walk | `v6/tsv2/runtime/1_incremental.ts:575`, `:715` |
| v6 | tsv2 emitter | the SQL text for all of the above | codegen | `v6/prolog/lower.pl:4534`, `:5391` |
| v6 | engine-rs | the same plan as one JSON blob | counts and a whole-head refill | `v6/sprefa-engine-rs/src/incremental.rs:2923` |
| v6 | store cascade | a graph in `cx_*` tables | four retraction shapes | `v6/sprefa-store/src/engine.rs:491`, `:559`, `:717`, `:812` |
| v6 | build floors | one `reachable` table | a build fixpoint, no upkeep | `v6/labs/exec_shootout/sqlite_baseline/src/engines.rs:58` |
| v7 | query emitter | a view declared over the extension | the extension | `v7/src/3_emit/1c_sqlite_query_emitter.pl:521` |
| v8 | eval engine | dictionary, seed tables, material tables, one view | the view for its own products, Rust rounds for the rest | `src/_6_eval/_7_sqlite_eval.rs:116`, `:130` |
| v8 | runtime store | the dictionary mirror and product rows | append only | `src/_9_runtime/_1_sqlite.rs:659` |
| live | the extension | one arrangement per operator | signed deltas, rederive for recursion | `~/projects/sqlite_ivm/src/1a_relational.rs:728` |

## The upkeep, one line per edge

v5 derived tables

- `derive.rs:334` rebuild_derived
  - wipes each dependency component just before it runs (`derive.rs:408`)
  - refills it with a semi-naive fixpoint (`derive.rs:1673`)
  - `tick.rs:1039` and `tick.rs:1162` call it on a tick

v5 call facts

- `storage/call.rs:248` apply_sqlite_call_owner_delta
  - gathers the affected keys for one owner
  - replaces that owner's sites as one batch

v5 test fixture

- `deltaflow.rs:116` apply_generation
  - deletes only the rows a changed candidate touches (`deltaflow.rs:429`)
  - the file says it never runs in production (`deltaflow.rs:3`)

v6 tsv2

- `1_incremental.ts:575` refCount reconcile
  - reseed the counts, subtract the difference, delete what hit zero, insert what came back
- `1_incremental.ts:715` maintain_head_in_place
  - a gate asks whether any delta is a removal (`1_incremental.ts:890`)
  - no removal means the plain insert half runs
  - a removal means the rederive half runs (`1_incremental.ts:799`, `:820`)
  - a walk that grows past a quarter of the head gives up and refills the whole head (`1_incremental.ts:801`, `:880`)
- `lower.pl:4534` writes the delete plus insert text per head
- `lower.pl:5391` writes the rederive text for a recursive head
- engine-rs carries the rederive plan as data only (`types.rs:485`), and its bench plans leave that slot empty (`bench/sf_join_per.rs:7`)

v6 store cascade

- `engine.rs:491` assert, forward add
- `engine.rs:559` retract_dred, a Rust round loop
- `engine.rs:717` retract_dred_cte, a recursive query
- `engine.rs:812` retract_signed_delta, one signed pass
- counting is wrong on a loop of rows, the file says so (`FINDINGS-AND-GAPS.md` section 2)

v7 and v8

- `1c_sqlite_query_emitter.pl:521` writes one `sqlite_ivm` view
- `_5_reify/_7_sqlite.rs:2203` writes one `sqlite_ivm` view for a whole rule set
- `_6_eval/_7_sqlite_eval.rs:116` empties the material tables and reruns the rounds on a removal

## Delete and rederive, plain words

A removal can kill rows that another row still justifies. The walk kills the
whole forward region first, then brings back what a surviving row still
justifies. v6 has it. v5 does not. v8 has it only where the extension owns the
rows.

| who | where the walk lives | what stops it |
|---|---|---|
| v6 store, round loop | `v6/sprefa-store/src/engine.rs:559` | the next wave is empty (`:616`, `:687`) |
| v6 store, recursive query | `engine.rs:717` | the query runs out of rows (`:792`) |
| v6 store, signed pass | `engine.rs:812` | no signed row at the current step (`:878`) |
| v6 tsv2 | `v6/tsv2/runtime/1_incremental.ts:715` | the wave is empty, or the region is too big and the head is refilled (`:848`, `:880`) |
| the extension | `~/projects/sqlite_ivm/src/1a_relational.rs:728` | the member table stops growing (`:788`) |
| v8 | the material tables have no walk | `src/_6_eval/_7_sqlite_eval.rs:116` empties them instead |

## The numbers

| era | where | what it says |
|---|---|---|
| v5 | `v5/tests/it/seminaive.rs:350` | naive 80 full re-runs against a budget of 20, semi-naive 0 |
| v6 | `v6/labs/exec_shootout/dl6/FACTS.dredland.md` | one edge in 2,019 ms to 42 ms, one edge out 3,907 ms to 56 ms, an empty tick 1,926 ms to 1 ms |
| v6 | `plans/2026-08-06-dred-emit-lab-header.md` | one edge in 40 ms against 2,179 ms, one edge out 60 against 2,195, a hundred edges out 7,153 against 2,218, and a bigger region loses outright |
| v6 | `v6/labs/exec_shootout/sqlite_baseline/BASELINE.md` | a plain sqlite build beats the old engine on time and memory in every case |
| v6 | `v6/labs/exec_shootout/sqlite_raw/REPORT.md` | the same for a plain sqlite build driven from JS |
| v6 | `v6/sprefa-store/PERF-REPORT.md` | counts are fast and wrong on loops, rederive is slower and right, dd is fastest and keeps everything in memory |
| v6 | `v6/labs/exec_shootout/postgres_pglite_ivm/29_sqlite_ivm_public.md` | a whole-result refill cost 8,354.893 ms where pg_ivm cost 23.646 ms |
| v6 | `v6/labs/exec_shootout/tick_bench/TICKS.md` | a tick costs 55.514 ms on mono against 214.253 on mercury, and the delta-proportional number was not built |
| v6 | sprefa commit `e2052d5ae`, `TASKS/7_sqlite_competitive.REPORT.md` | eleven sqlite circuits with negations and a cyclic removal all pass; DD leads the timing table |
| v7 | `v7/receipts/0_sqlite_query.md` | a statement cache took an update from 283.269 ms to 58.394 ms |
| v8 | none | only oracle counts exist; no timing was taken |
| live | `~/projects/sqlite_ivm/bench/46_feature_acceptance.md` | a loaded extension matches an independent sqlite on every checked state |

## What v8 has now

- A removal empties every material table and reruns the rounds
  (`src/_6_eval/_7_sqlite_eval.rs:116`).
- The file admits the removal path disagrees with a fresh view when the check
  flag is on (`:1095-1096`, the flag read at `:1156`).
- The rounds stop at a cap (`:44`).
- The in-memory table only appends, it cannot take a row back
  (`src/_6_eval/_3_table.rs:27`).
- The lane that would have added a removal path was written down
  (`plans/v8/2026-09-14-v8-retraction.brief.md`) and its files are not in the
  tree: no `tests/_19_retraction.rs`, no `fixtures/retraction`, no
  `bench/retraction`.

## The answer

An earlier era did better. v6 could take a row back from a recursive product
without refilling everything, and the code is above. v8 does that only for the
rows the extension owns; for a product whose rule reads itself twice, v8
refills. The fix is small and already designed: give the material tables a
delete step beside their insert step, walk the region, and stop at the cap that
is already in the file.
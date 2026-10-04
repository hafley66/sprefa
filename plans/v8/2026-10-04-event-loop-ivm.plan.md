# One event loop on IVM: to-do for correctness (2026-10-04)

User: "what is to do list for full event loop ivm correctness. no more runtime, the sqlite store is
the ivm set of things/storage? or do we just need different aspects of dd/sqlite that overlap".

## Today (verified 2026-10-04)

- `Reconciler::run` (`src/_9_runtime/_2_reconcile.rs:217-252`): evaluate, poll executors, block on
  armed ones (`fs.watch`, `timer`), repeat; exits when nothing is armed and nothing new, or at
  `--max-ticks` (unset = `usize::MAX`).
- Every tick `evaluate_tick` (`:355`) -> `Evaluate::reduce(all rows)` -> `evaluate_ir` ->
  `run_fresh` -> `open_session` (`_9b_ir_eval.rs:1166`): a fresh install with every stored row as
  a seed, per tick.
- One live engine exists only on `dl8 run --frontiers -` (`run_live`, `:282`).
- Persistence: `SqliteRowStore` file (`_9_runtime/_1_sqlite.rs`), separate from the engine; with
  ir:sqlite the rows also sit in the engine's own in-memory SQLite.
- Executors picked by relation-name string (`const RELATION: &str` in each `_3_executors/*.rs`).

## Storage answer: persist inputs, derive the rest

| data | kind | where |
|---|---|---|
| source facts (files, ryi facts, seeds) | input | store (later: content-addressed segments + manifest) |
| effect answers (timer fires, fetch bodies, fs.watch bodies, git refs) | input: not reproducible | store, append with their epoch |
| derived relations | output | engine state only. ir:sqlite may keep it in the same file (`Sqlite::install_on(Connection)`) for a warm restart; ir:dd rebuilds it from inputs on start |

So the store is the set of inputs; the engine owns derived state. One copy of each fact.

## To-do (one fresh agent per item, sequential)

1. One live engine for the whole run: `Reconciler::run` drives `run_live` for every executor, not
   only `--frontiers -`. Each tick sends only new answer rows.
2. Effects as signed changes: `fs.watch` = retract the old body, assert the new; `git.refs` moved
   ref = retract old, assert new; `timer` appends; `fetch_json` replaces. Each executor declares
   its change shape.
3. One tick = one epoch: every answer that arrives in one poll slice is one frontier, settled once,
   persisted in one transaction (`persist` already opens one per tick, `:372`).
4. Store holds inputs only: sources + answers with epoch; derived rows are not written by
   `persist`. Restart = read inputs, install once, settle once.
5. Warm restart on ir:sqlite: install on the store file's connection so derived state survives;
   check equality with a cold rebuild.
6. Executors by identity: an executor is bound to a relation ref (`ref(effect(..))`), not a name
   string (same fix as kernel-by-identity).
7. Compile and run in one loop: source edits are inputs too. Until macrotime/comptime pages 2-4
   land, a source edit that changes the program shape reinstalls; one that does not is a frontier.

## Correctness tests (the definition of done)

- Incremental == fresh: after every tick, the live engine's closure equals a fresh install over
  the same inputs (pattern exists: `_2_eval_oracles::random_seed_retractions_match_fresh_installs_on_both_engines`).
- Restart == uninterrupted: kill after tick k, restart, continue; final store and closure equal an
  uninterrupted run.
- ir:dd == ir:sqlite per tick.
- Retraction: an fs.watch edit that removes a fact removes every derived row that depended on it.
- Exactly-once effects: a restart does not re-answer an answered request (exists:
  `a_second_run_against_the_db_answers_nothing_and_inserts_nothing`).

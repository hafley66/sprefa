# v9 walkthrough (2026-10-25)

Branch `v9`, PR #821. Crate `v9/`. Read top to bottom.

## 1. What got done

| step | what landed | proof |
| --- | --- | --- |
| P1 | Rust builder for programs (`_2_ir.rs`), `check` + `stratify` to a `Plan` (`_3_plan.rs`), `V9_SYSTEM` recipe (`_4_recipe.rs`), the end traits (`_5_ends.rs`) | `tests/1_plan.rs`, `tests/2_recipe.rs` |
| P2 | `Engine` on differential dataflow (`_6_back/`): joins, negation, `add`/`lt`/`neq`, recursion, round limit with rollback | `tests/3_engine.rs`: 12 cases + 4000-node random graph, each step equal to a naive oracle |
| P3 | `Store` on SQLite (`_7_store/`): one table per relation, `dict`, `meta`, `<rel>_text` views | `tests/4_store.rs` |
| P4 | `Core` (`_8_runtime/_3_core.rs`): commit -> step -> write -> services -> repeat; `Ryi` service; `v9 serve` | `tests/5_runtime.rs` |
| P5a/P5b | HTTP routes and OpenAPI generated from `schema/0_wire.tsp` | `tests/6_wire.rs`, `node schema/1_generate.mjs --check` |
| P6 | `v9 reach` runs the reach program on Core; skeleton deleted | hafley_scm: 4737 edges, 24322 reach, equal to skeleton and oracle |
| P7 | `_5_run::serve(program, ...)` for your own program; 3 examples; `Core::refresh`; README | `tests/8_examples.rs` |
| P8 | round limit counts per event per stratum | `tests/3_engine.rs` service-chain test |
| review | fresh reviewer tried to break it: 17 findings, all reproduced | review report in this session |
| F1/F2 | atomic commits, set-valued client commits, persisted event ids, query row cap + time budget, ryi timeout; 422/500 in contract, effect gates, loop-law scanner, doc fixes | `tests/5_runtime.rs` (17), `tests/9_laws.rs` |
| hafley-tsp | emitter: unions, keyword fields, handler trait, module prefix, error status, 422/500 | `emitter-routes` d37de67, 9cb2235 (local, not pushed) |

## 2. How one commit flows (the dataflow)

```
client JSON  --POST /commit-->  _9_wire/http_auto.rs (generated)
  -> _8_runtime/_4_handle.rs     Wire impl; sends to the Core thread
  -> _8_runtime/_3_core.rs       commit: check cells, sum per row, keep only real set changes
  -> _6_back/_1_dd.rs  step      feed DD input, run worker until the probe passes the epoch
       _6_back/_0_rule.rs        each rule = join / antijoin / filter / map over binding rows
  -> back in Core: Request rows -> IService::call -> poll -> Answer rows -> step again (<= 64 rounds)
  -> _7_store/_1_sqlite.rs write one transaction for the whole commit
  -> Delta back to the client
failure anywhere: step the engine back by everything fed, store nothing, reseed services
```

## 3. First five files to read (dataflow order)

| # | file | read | why |
| --- | --- | --- | --- |
| 1 | `v9/examples/0_reach.rs` | whole (short) | a dl9 program in Rust, lisp form above each line |
| 2 | `v9/src/_3_plan.rs` | `check` at :73, `stratify` at :410 | names -> ids, rules -> binding slots, SCC strata |
| 3 | `v9/src/_6_back/_0_rule.rs` | `lower` :26, `pos` :53 | one rule becomes DD operators |
| 4 | `v9/src/_6_back/_1_dd.rs` | `step` :177, `build` :235 | the engine: epochs, iterate scopes, round limit, rollback |
| 5 | `v9/src/_8_runtime/_3_core.rs` | `commit` :162, `rounds` :306, `settle` :327, `step_back` :382 | the runtime loop: services, atomic write, undo |

## 4. How bad the slop is

`plans/v9/SLOP.md`: 45 rows. 13 closed by later punches. 32 open, sorted:

| bucket | count | rows |
| --- | --- | --- |
| can give wrong state | 4 | round limit still climbs with chain growth across events (P4 row reopened by F2); `StepLimit` leaves the engine broken until restart (P2); a step-back that itself hits a bound rebuilds from the store and can then disagree (F1); rollback equality checked only in debug builds (P2) |
| blocks clients | 1 | ryi runs on the Core thread, up to 60 s per root (F1) |
| speed / memory | 4 | each rel filters the one input stream; rows are `Vec<u64>`; whole `dict` loaded at open; hafley_scm engine step 58-60 ms vs skeleton 16, warm peak ~62 MB vs 35 |
| wire / API shape | 6 | Diag sent as Debug text; `Option<Cell>`; every body error is 422; service errors are strings; `Plan::hash` from Debug text; emitter linked by path, not a version |
| process / cosmetic | 17 | edits outside owned files, `println!` in `src/`, Debug-struct diag in the bin oracle, empty strata, etc. |

One row is stale: the F2 row saying the 422/500 emitter lives only on `e2-rejections`; it was fast-forwarded onto `emitter-routes`.

## 5. Review depth

Each punch: the driver's report, the main changed file read in full, its tests rerun, and a smoke run where there was a binary. The navigator did not read every line: of the 13815 inserted lines, many are snapshots, generated wire code and lock files. The deep pass was the separate whole-branch reviewer (verified 17 findings by running them).

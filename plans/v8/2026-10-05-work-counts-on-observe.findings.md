# Findings: sqlite work counts on hafley-observe; growth assertions

Brief: `plans/v8/2026-10-05-work-counts-on-observe.brief.md`. Lane `feat/observe-work-counts`.

## Mapping: `ivm-sqlite/src/0_work.rs` count -> observe facility

| count | facility | status |
|---|---|---|
| vm_steps, fullscan_steps, sorts, autoindexes, reprepares | `sqlite::StatementCounters::take_with` (shared with `instrument`) | existing, extended: `take_with(stmt, reset_run)` keeps RUN unreset in work mode |
| executions | `sqlite_work::WorkTrace` PROFILE count | new |
| prepared, prepared_bytes | `sqlite_work::WorkTrace`, RUN status 0 at STMT | new |
| unfolded_bytes | `sqlite_work::unfolded_bytes` | new (moved) |
| creates | `sqlite_work::target` verb `Create` | new (moved) |
| schema_rows_parsed | `sqlite_work::WorkTrace`, ROW events with null SQL per schema | new (moved) |
| rows_returned | `sqlite_work::WorkTrace`, ROW events | new |
| zero_row_executions | `sqlite_work::WorkTrace` | new |
| rows written | `sqlite_work::Work::written` (per table name), `Work::written_where` | new, generic |
| written_d / _i / _x / _scratch / _source / _catalog / _term | `ivm-sqlite` `kind(table)` over `written_where` | stays in ivm-sqlite (table naming is ivm-specific) |
| `statement begins` / `statement finished` / finding warnings | `sqlite::emit_begin`, `sqlite::emit_finished` | existing, split out of `emit`; `WorkTrace` emits them too |
| growth of a plain count | `growth_between`, `assert_growth_at_most`, `Growth: Ord` | new in `4_counts.rs` |
| interval reporting | `WorkTrace::interval` | new |

## What moved

- hafley-observe `src/5b_sqlite_work.rs` (module `sqlite_work`): `Work`, `WorkTrace` (trace_v2
  STMT|PROFILE|ROW, unregistered on drop), `target`, `unfolded_bytes`, `MAX_STATEMENT_BYTES`.
- hafley-observe `src/5_sqlite.rs`: `emit` split into `statement_sql`, `emit_begin` (expanded SQL
  computed only when the TRACE callsite is enabled), `emit_finished`; `StatementCounters::take_with`.
- hafley-observe `src/4_counts.rs`: `growth_between` (the midpoint classifier `observed_growth`
  now delegates to), `assert_growth_at_most`, `Growth` derives `PartialOrd, Ord`.
- ivm-sqlite `src/0_work.rs`: 519 -> 167 lines. Keeps the 20-field `Work` (pinned Debug shape),
  `From<&sqlite_work::Work>`, `log` (the `ivm_sqlite::work` event, same field order),
  `WorkLog` (`WorkTrace` + `report(phase)` + `between` on drop), `kind`.
- ivm-sqlite `tests/6_install_growth.rs`: its own `sqlite3_trace_v2` CREATE counter replaced by
  `sqlite_work::WorkTrace::creates`; `pure_delta_chain_has_fixed_create_count` still reads 19.

The SQL tokenizer moved unchanged (equal values required). Candidates to replace it, not tried here:

| candidate | crate | note |
|---|---|---|
| sqlite3-parser | `sqlite3-parser` 0.17 (lemon-rs) | already a dependency of the sqlite_ivm root crate |
| sqlparser | `sqlparser` (SQLite dialect, spans) | CTE spans needed for unfolded_bytes |
| tree-sitter-sql | `tree-sitter-sequel` | byte ranges per node |

## Counts equal before/after

| pin | lines | result |
|---|---|---|
| `ivm-sqlite/tests/13_work_counts.rs::registry_install_work` | 8 | equal (test passes unchanged) |
| `ivm-sqlite/tests/14_nested_letrec.rs::work_counts` | 4 | equal (test passes unchanged) |
| sprefa `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts` | 3 phases | equal (test passes unchanged) |

## Growth: `ivm-sqlite/tests/15_work_growth.rs`

Family: n reach rules over one edge source (n + 1 relations, n LetRec strata, 5n nodes), one install
and one settle of an 8-edge chain. Sizes n -> 4n for n = 4 and n = 16. SQLite counts are field sums
of the `ivm_sqlite::work` events captured by `CountRecorder::event_sums`; DD counts are
`Engine::counters()`. Class by `growth_between` (ratio 4: Linear from 2.5x, Quadratic from 10x).
`#[oh::test(memory_bytes = 268435456)]` with `oh::counting_allocator!()`. Rows with zero at all
sizes omitted.

| engine | phase | count | n=4 | n=16 | class 4->16 | n=16 | n=64 | class 16->64 |
|---|---|---|---|---|---|---|---|---|
| sqlite | install | creates | 71 | 228 | Linear | 228 | 852 | Linear |
| sqlite | install | executions | 93 | 250 | Linear | 250 | 874 | Linear |
| sqlite | install | fullscan_steps | 10 | 10 | Constant | 10 | 10 | Constant |
| sqlite | install | prepared | 93 | 250 | Linear | 250 | 874 | Linear |
| sqlite | install | prepared_bytes | 12475 | 38071 | Linear | 38071 | 141068 | Linear |
| sqlite | install | rows_returned | 20 | 20 | Constant | 20 | 20 | Constant |
| sqlite | install | schema_rows_parsed | 405 | 3084 | Linear | 3084 | 44190 | Quadratic |
| sqlite | install | sorts | 5 | 17 | Linear | 17 | 65 | Linear |
| sqlite | install | unfolded_bytes | 12475 | 38071 | Linear | 38071 | 141068 | Linear |
| sqlite | install | vm_steps | 2430 | 6838 | Linear | 6838 | 24358 | Linear |
| sqlite | install | written | 5 | 5 | Constant | 5 | 5 | Constant |
| sqlite | install | written_catalog | 4 | 4 | Constant | 4 | 4 | Constant |
| sqlite | install | written_term | 1 | 1 | Constant | 1 | 1 | Constant |
| sqlite | install | zero_row_executions | 1 | 1 | Constant | 1 | 1 | Constant |
| sqlite | settle | executions | 510 | 1941 | Linear | 1941 | 7653 | Linear |
| sqlite | settle | fullscan_steps | 2431 | 9698 | Linear | 9698 | 38738 | Linear |
| sqlite | settle | prepared | 108 | 375 | Linear | 375 | 1431 | Linear |
| sqlite | settle | prepared_bytes | 18987 | 67302 | Linear | 67302 | 265955 | Linear |
| sqlite | settle | rows_returned | 189 | 709 | Linear | 709 | 2821 | Linear |
| sqlite | settle | sorts | 217 | 837 | Linear | 837 | 3333 | Linear |
| sqlite | settle | unfolded_bytes | 17786 | 64315 | Linear | 64315 | 253890 | Linear |
| sqlite | settle | vm_steps | 101079 | 395149 | Linear | 395149 | 1576423 | Linear |
| sqlite | settle | written | 2505 | 9961 | Linear | 9961 | 39721 | Linear |
| sqlite | settle | written_catalog | 1 | 1 | Constant | 1 | 1 | Constant |
| sqlite | settle | written_d | 1440 | 5776 | Linear | 5776 | 23056 | Linear |
| sqlite | settle | written_i | 472 | 1864 | Linear | 1864 | 7432 | Linear |
| sqlite | settle | written_scratch | 584 | 2312 | Linear | 2312 | 9224 | Linear |
| sqlite | settle | written_source | 8 | 8 | Constant | 8 | 8 | Constant |
| sqlite | settle | zero_row_executions | 79 | 308 | Linear | 308 | 1220 | Linear |
| dd | settle | rows_written | 144 | 576 | Linear | 576 | 2304 | Linear |
| dd | settle | delta_rows | 224 | 896 | Linear | 896 | 3584 | Linear |
| dd | settle | delta_rows_join | 112 | 448 | Linear | 448 | 1792 | Linear |
| dd | settle | rounds | 32 | 128 | Linear | 128 | 512 | Linear |

Gate result: `work_grows_linear_or_slower` fails at
`n=16 sqlite install schema_rows_parsed grew Quadratic from 3084 to 44190 across a 4x input, ceiling Linear`
(14.3x at 4x; 7.6x at n=4). Per CREATE, the schema reparse scans the target schema's
`sqlite_master`; creates grow linearly and each shard's `sqlite_master` grows linearly, so the
product is quadratic. Every other gated count (install creates, install/settle prepared_bytes,
settle vm_steps, settle written, dd rows_written / delta_rows / rounds) is Linear or Constant at
both sizes. The test is left red on this count, as the brief specifies.

## Overlap with parallel lanes

- `feat/rule-interpreter` (sprefa): edits `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts`
  (adds `DL8_RULES=compile`, rustfmt). This lane does not edit that file.
- `feat/macrotime-waves-engine`: no edits to `0_work.rs`, `4_engine.rs`, hafley-observe, or the pins.

## Pre-existing, not from this lane

- hafley-rs `sqlite-ext --test 2_load_plugins`: 3 failures, `cannot update the lock file
  crates/sqlite-ext/tests/fixtures/Cargo.lock because --locked was passed` (fixture lock last
  touched 2026-09-27; this lane changes no Cargo.toml in hafley-rs).

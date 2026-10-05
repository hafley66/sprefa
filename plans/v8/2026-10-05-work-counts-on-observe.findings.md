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
both sizes. The test was left red on this count in the first commit; the next section makes it Linear.

## Overlap with parallel lanes

- `feat/rule-interpreter` (sprefa): edits `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts`
  (adds `DL8_RULES=compile`, rustfmt). This lane does not edit that file.
- `feat/macrotime-waves-engine`: no edits to `0_work.rs`, `4_engine.rs`, hafley-observe, or the pins.

## Pre-existing, not from this lane

- hafley-rs `sqlite-ext --test 2_load_plugins`: 3 failures, `cannot update the lock file
  crates/sqlite-ext/tests/fixtures/Cargo.lock because --locked was passed` (fixture lock last
  touched 2026-09-27; this lane changes no Cargo.toml in hafley-rs).

## Install schema_rows_parsed made Linear (coordinator follow-up)

Cause: SQLite reparses after every CREATE by scanning the target schema's `sqlite_master`; with 8
fixed shard schemas each shard holds objects/8, so the reparse total is quadratic in objects.
Family probe after install (rows of `sqlite_master` per schema): `main` 22 at every n; each of
the 8 shards 7 / 27 / 105 / 417 at n = 4 / 16 / 64 / 256.

Arms measured (work counts, family n -> 4n, registry pins):

| arm | n=4 install schema_rows_parsed | 16 -> 64 class | settle vm_steps vs before | kept |
|---|---|---|---|---|
| before: 8 shards, round-robin | 405 | Quadratic (3084 -> 44190) | - | - |
| fill shards to 32 objects, then open the next | 976 | Linear | not measured | no (small programs parse 2.4x) |
| at least 8 shards, ceil(objects / 32) shards, round-robin | 405 | Linear (3084 -> 14142) | n=64: +148; registry: +2004 (+0.005 %); a statement over tables in more schemas runs one more `Transaction` op per schema | no |
| at least 8 shards, ceil(objects / 32) shards, consecutive tables per shard | 424 | Linear (3110 -> 13650) | n=64: -6407; registry: -9712 (-0.02 %) | yes |
| A3 node-keyed shared tables (no DDL per node) | not built | | | rewrites every settle statement in `3_nodes.rs` |

Kept: `catalog.rs` counts the shardable objects of the install first, opens
`min(SHARDS, max(MIN_SHARDS = 8, ceil(objects / SHARD_OBJECTS = 32)))` shard schemas, and places
consecutive tables in the same shard (indexes follow their table). `1_image.rs::install_shards`
attaches exactly the shards the DDL fills (before: `4_engine.rs` attached 8 up front).
`SHARDS` = 125, lowered at install to the connection's `SQLITE_LIMIT_ATTACHED`.

Ceiling: SQLite's compile-time `SQLITE_MAX_ATTACHED` (default 10, at most 125). Both
`sqlite_ivm/.cargo/config.toml` and sprefa `.cargo/config.toml` set
`LIBSQLITE3_FLAGS = "SQLITE_MAX_ATTACHED=125"` for the bundled build. Linear up to
125 x 32 = 4000 shard objects; above that per-shard objects grow again and the reparse total is
quadratic (/125). A SQLite built without the flag (default 10, or a host loading the extension)
gets 10 shards: Linear up to 320 shard objects. Registry comptime: 2953 CREATEs.

### Family counts (`15_work_growth`, n = 4 / 16 / 64 / 256; before = 8 round-robin shards)

| count | before n=4 | before 16 | before 64 | after n=4 | after 16 | after 64 | after 256 | after class 64 -> 256 |
|---|---|---|---|---|---|---|---|---|
| install creates | 71 | 228 | 852 | 71 | 228 | 852 | 3348 | Linear |
| install schema_rows_parsed | 405 | 3084 | 44190 | 424 | 3110 | 13650 | 55987 | Linear |
| install prepared | 93 | 250 | 874 | 93 | 250 | 893 | 3467 | Linear |
| install vm_steps | 2430 | 6838 | 24358 | 2414 | 6822 | 24532 | 95388 | Linear |
| install rows written | 5 | 5 | 5 | 5 | 5 | 5 | 5 | Constant |
| settle prepared | 108 | 375 | 1431 | 108 | 375 | 1431 | 5655 | Linear |
| settle vm_steps | 101079 | 395149 | 1576423 | 100754 | 393549 | 1570016 | 6276404 | Linear |
| settle rows written | 2505 | 9961 | 39721 | 2505 | 9961 | 39721 | 158761 | Linear |

`15_work_growth` now checks n = 4, 16 and 64 against 4n and passes; every gated count is Linear
or Constant on both engines (dd counts unchanged).

### Registry and census totals

Registry (`tsi_registry_sqlite_work_counts`, pins updated; `between` unchanged):

| count | before | after |
|---|---|---|
| install creates | 4963 | 4963 |
| install schema_rows_parsed | 620094 | 106435 (-82.8 %) |
| install prepared | 5290 | 5411 (+121 ATTACH) |
| install vm_steps | 148531 | 149691 (+0.8 %) |
| install rows written | 166 | 166 |
| settle prepared | 5694 | 5694 |
| settle vm_steps | 40959483 | 40949771 (-0.02 %) |
| settle rows written | 454967 | 454967 |

`13_work_counts` registry corpora, install schema_rows_parsed before -> after: macro_library
58028 -> 19688, macrotime 59499 -> 20606, registry_comptime 502567 -> 66141, c15 348026 -> 65534;
empty settles equal except c15 vm_steps 404 -> 400. `14_nested_letrec` settles vm_steps
43831 -> 43486 and 50167 -> 49729.

Census: every `.dl7` under `fixtures/ examples/ book/src/probes/ std/` (132 files), `dl8 compile`,
ir:sqlite, `RUST_LOG=ivm_sqlite::work=info`, sums over the 112 that exit 0 with both binaries
(the same 112):

| count | before | after |
|---|---|---|
| install creates | 388293 | 388293 |
| install schema_rows_parsed | 27172309 | 8746794 (-67.8 %) |
| install prepared | 424524 | 432880 (+2.0 %, ATTACH) |
| install vm_steps | 11880441 | 11958529 (+0.7 %) |
| install rows written | 18402 | 18402 |
| settle prepared | 199950 | 199950 |
| settle vm_steps | 936009605 | 935778269 (-0.02 %) |
| settle rows written | 10089998 | 10089998 |
| between (all five) | equal | equal |

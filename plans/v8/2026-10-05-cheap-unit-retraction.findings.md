# Findings: cheaper retraction between macrotime units (ir:sqlite)

Brief: `plans/v8/2026-10-05-cheap-unit-retraction.brief.md`. Base sprefa `676f4c499`, sqlite_ivm
`3f7eb15` (clone inside the worktree, branch `perf/cheap-unit-retraction`, after: `db02a77`). Debug binaries
(`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`), `RCARGO_OFF=1`.

## Where the retraction settle spends time (before)

Command (traced; the chrome timeline costs about 30% wall, so ms below are inflated and are for
proportions):

```
F="dl8::macrotime=info,frontier=debug,sqlite_ext=debug,ivm_sqlite=debug"
HAFLEY_TRACE=<tmp>/base.json HAFLEY_TRACE_FILTER="$F" RUST_LOG="$F" DL8_ENGINE=ir:sqlite \
  dl8 build std/tsi_registry_emit.dl7 --out <tmp>
```

The window of a unit's enter settle runs from the previous `macrotime unit` event to this unit's
`macrotime enter` event. Every settle statement runs under a `stmt` span; its SQL comes from the
nested `prepare` span and its row count from the `execute_end` event. Phase is the enclosing
`frontier_fixpoint` / `frontier_round deleting=` span (`3_nodes.rs` `fixpoint`, `round`): outside a
fixpoint = non-recursive nodes; `deleting=true` rounds = DRed over-delete; fixpoint statements
outside rounds = stash, seeds, rederive, restore, finish. Role is the suffix of the table written
(`_d` delta fill, `_i` integrated image, `_nx`, `_acc`, `_del`, `_hold`); "read" is a query.
Source-row writes (`4_engine.rs` `settle`, one `INSERT`/`DELETE` per row) carry no span; they are
in "outside frontier_settle" with the decode of the delta.

Rows sent in the enter settle: unit 2 = 3928 retracted + 1315 added, unit 3 = 552 + 877,
unit 4 = 396 + 9834 (`sent` 5243, 1429, 10230).

### unit 2 enter settle: window 165.1 ms; frontier_settle 148.8 ms; outside it (source writes, decode) 16.3 ms; 1302 statements 136.7 ms

| phase | statements | rows | ms |
|---|---|---|---|
| non-recursive | 235 | 42331 | 74.0 |
| DRed over-delete rounds | 413 | 61528 | 41.5 |
| DRed insert rounds | 377 | 7859 | 12.0 |
| DRed rederive/seed/finish | 277 | 11180 | 9.2 |

| phase x role | statements | rows | ms |
|---|---|---|---|
| non-recursive / integrate image (del) | 17 | 10685 | 20.2 |
| non-recursive / integrate image (ins) | 17 | 13196 | 19.9 |
| non-recursive / fill delta (ins) | 49 | 4235 | 13.4 |
| DRed over-delete rounds / fill delta (ins) | 106 | 9930 | 13.0 |
| output delta read (`3_nodes.rs` `run`) | 1 | 0 | 11.6 |
| DRed over-delete rounds / integrate image (del) | 44 | 14099 | 10.9 |
| DRed over-delete rounds / integrate image (ins) | 44 | 14099 | 10.7 |
| non-recursive / read | 151 | 14215 | 8.9 |
| DRed insert rounds / fill delta (ins) | 106 | 1775 | 4.3 |
| DRed rederive/seed/finish / fill delta (ins) | 85 | 3663 | 3.4 |
| DRed insert rounds / integrate image (ins) | 44 | 2562 | 2.4 |
| DRed over-delete rounds / stage next (ins) | 36 | 2694 | 1.9 |
| DRed insert rounds / integrate image (del) | 44 | 0 | 1.7 |
| DRed rederive/seed/finish / stash (ins) | 47 | 580 | 1.7 |

### unit 3 enter settle: window 63.8 ms; frontier_settle 58.2 ms; outside it (source writes, decode) 5.6 ms; 1205 statements 52.3 ms

| phase | statements | rows | ms |
|---|---|---|---|
| non-recursive | 138 | 19672 | 20.1 |
| DRed over-delete rounds | 413 | 10237 | 14.5 |
| DRed insert rounds | 377 | 6257 | 11.5 |
| DRed rederive/seed/finish | 277 | 3405 | 6.2 |

| phase x role | statements | rows | ms |
|---|---|---|---|
| non-recursive / integrate image (ins) | 17 | 3311 | 5.1 |
| non-recursive / integrate image (del) | 17 | 1556 | 4.6 |
| DRed over-delete rounds / fill delta (ins) | 106 | 1775 | 4.6 |
| non-recursive / fill delta (ins) | 49 | 1414 | 4.6 |
| DRed insert rounds / fill delta (ins) | 106 | 1472 | 3.9 |
| non-recursive / read | 54 | 13391 | 3.0 |
| output delta read (`3_nodes.rs` `run`) | 1 | 0 | 2.8 |
| DRed over-delete rounds / integrate image (del) | 44 | 2371 | 2.8 |
| DRed over-delete rounds / integrate image (ins) | 44 | 2371 | 2.7 |
| DRed rederive/seed/finish / fill delta (ins) | 85 | 1093 | 2.4 |
| DRed insert rounds / integrate image (ins) | 44 | 2031 | 2.2 |
| DRed insert rounds / integrate image (del) | 44 | 0 | 1.7 |
| DRed insert rounds / read | 75 | 1887 | 1.3 |
| DRed over-delete rounds / read | 75 | 2164 | 1.3 |

### unit 4 enter settle: window 190.6 ms; frontier_settle 142.4 ms; outside it (source writes, decode) 48.2 ms; 1376 statements 131.2 ms

| phase | statements | rows | ms |
|---|---|---|---|
| non-recursive | 306 | 55233 | 74.3 |
| DRed insert rounds | 380 | 37715 | 33.3 |
| DRed over-delete rounds | 413 | 8199 | 15.1 |
| DRed rederive/seed/finish | 277 | 19515 | 8.5 |

| phase x role | statements | rows | ms |
|---|---|---|---|
| non-recursive / integrate image (ins) | 16 | 18255 | 20.0 |
| non-recursive / integrate image (del) | 16 | 1147 | 16.3 |
| non-recursive / fill delta (ins) | 49 | 10438 | 14.6 |
| non-recursive / read | 224 | 25393 | 12.9 |
| output delta read (`3_nodes.rs` `run`) | 1 | 0 | 10.5 |
| DRed insert rounds / fill delta (ins) | 106 | 7747 | 9.1 |
| DRed insert rounds / integrate image (ins) | 45 | 12394 | 7.7 |
| DRed insert rounds / accumulate (ins) | 36 | 2066 | 7.2 |
| DRed insert rounds / integrate image (del) | 45 | 0 | 5.0 |
| DRed over-delete rounds / fill delta (ins) | 106 | 1472 | 4.9 |
| DRed rederive/seed/finish / fill delta (ins) | 85 | 5984 | 4.0 |
| DRed over-delete rounds / integrate image (del) | 44 | 1905 | 2.7 |
| DRed over-delete rounds / integrate image (ins) | 44 | 1905 | 2.6 |
| DRed insert rounds / stage next (ins) | 36 | 2066 | 1.8 |


### What the tables say about each lever

- Unit 2's enter settle sends 3928 retractions and 1315 insertions; the statements it runs touch
  42331 rows in non-recursive nodes and 61528 in DRed over-delete rounds, against 7859 in the
  insert rounds. The retraction is spread over every arrangement the unit's rows reached: the
  `_i` image deletes and re-inserts of the non-recursive nodes (40.1 ms) and the over-delete
  rounds (41.5 ms) are the bulk, rederive is 9.2 ms.
- DRed fast path (skip rederive): bounded by the rederive/seed/finish phase, 9.2 of 136.7 ms of
  statements in unit 2, 6.2 of 52.3 in unit 3, 8.5 of 131.2 in unit 4.
- Batching: settle statements are already set-based (1302 statements for 5243 source changes,
  each fill/integrate one statement per node). Per-row work is only the source writes, one
  `INSERT`/`DELETE` per row in `4_engine.rs` `settle`; they sit inside the 16.3 ms outside
  `frontier_settle` together with the decode.
- Unit-scoped rows: a unit key on every derived row turns each `_i` image delete into a
  partition delete, but every arrangement (stash, `_acc`, `_del`, image) still needs its own
  statement, the key would join through every node, and the plan compiler would change for
  every program, not only macrotime.
- No retraction at all: the expander reads only `net`, the protocol rows above `base`; nothing
  of unit k is read by unit k+1. The state the next unit needs is the state right after `base`.
  SQLite can return a connection to a savepoint by restoring the journaled pages, with no
  statement per relation and no rows re-derived.

Chosen: the last one, as an engine capability. `Engine::mark` records the state after `base`,
`Engine::rewind` returns to it; ivm-sqlite implements them with one savepoint
(`SAVEPOINT ivm_engine_mark` / `ROLLBACK TO ivm_engine_mark`). ivm-dd answers
`rewinds() == false` and keeps the retraction settle (its retraction costs about what its old
6 ms install did, see the page-2 findings).

## After

### Enter settles (same traced command, after)

| unit | enter window ms before / after | statements before / after | over-delete rows before / after |
|---|---|---|---|
| 2 | 165.1 / 45.7 | 1302 / 922 | 61528 / 0 |
| 3 | 63.8 / 37.2 | 1205 / 964 | 10237 / 0 |
| 4 | 190.6 / 162.4 | 1376 / 1098 | 8199 / 0 |

The `ROLLBACK TO` runs through `Connection::execute_batch` without a `stmt` span; it is inside
the "outside frontier_settle" column (unit 2: 16.3 ms before, 10.6 ms after, which also no
longer writes 3928 source deletes). The over-delete rounds still run their 134 empty-guarded
statements per settle (1.7 ms) because the waves inside a unit still retract rows.

### Untraced, interleaved before/after, 3 runs

Load 2.7-3.0 (`uptime` before and after the runs). `cmp` of the registry against
`hafley_scm/src/read/tsi/registry.rs` identical on every run of both binaries and engines.

| measure | ir:sqlite before | ir:sqlite after | ir:dd before | ir:dd after |
|---|---|---|---|---|
| unit 1 `unit_us` ms | 317, 295, 299 | 303, 295, 291 | 64, 63, 65 | 64, 64, 63 |
| unit 2 | 133, 128, 128 | 28, 28, 27 | 22, 22, 22 | 22, 23, 25 |
| unit 3 | 39, 34, 35 | 21, 21, 20 | 11, 11, 11 | 11, 13, 13 |
| unit 4 | 155, 150, 147 | 147, 147, 144 | 31, 30, 31 | 31, 32, 33 |
| macrotime sum ms | 644, 607, 609 | 499, 491, 482 | 128, 126, 129 | 128, 132, 134 |
| registry build real s | 1.89, 1.87, 1.89 | 1.77, 1.77, 1.76 | 0.37, 0.35, 0.36 | 0.37, 0.37, 0.37 |
| registry build max RSS MB | 244, 231, 249 | 242, 221, 231 | 105, 119, 116 | 114, 119, 119 |

ir:dd runs the same code path as before (`rewinds() == false`, `retire="retract"`); its spread
is run-to-run noise.

### Suites

| suite | result | wall |
|---|---|---|
| dl8 `cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1`, `DL8_ENGINE=ir:sqlite` | 166 passed, 0 failed | 247 s (incl. build) |
| same, `DL8_ENGINE=ir:dd` | 166 passed, 0 failed | 125 s |
| `ivm-sqlite` (`-p ivm-sqlite --test-threads=1`, fresh `TMPDIR`) | 112 passed, 9 ignored (111 + `rewind_returns_relations_and_dictionary_to_the_mark`) | 35 s |

The first ir:sqlite run failed 27 `_4_sqlite`/`_5_store_host` tests with `Invalid path:
sqlite_ivm/target/extension/release/libsqlite_ivm.dylib`: the fresh clone had no extension.
`cargo build --release --no-default-features --features extension --target-dir
target/extension` in the clone, then the rerun above. `every_macrotime_case_matches_v7`,
`macrotime_cases_share_one_engine`, `macrotime_reprepares_for_an_undeclared_shape` and
`tsi_registry_matches_hafley_scm` pass on both engines.

## How it works

### Engine API (`sqlite_ivm/crates/ivm-engine/src/1_rel.rs`)

```rust
fn rewinds(&self) -> bool { false }
fn mark(&mut self) -> Result<(), EngineError>;   // default Err(Unsupported("mark"))
fn rewind(&mut self) -> Result<(), EngineError>; // default Err(Unsupported("rewind"))
```

`mark` records the state of every relation, term and text; `rewind` returns to it and keeps the
mark, so one mark serves every later unit. Cells interned after the mark are freed and may be
reissued.

### ivm-sqlite (`4_engine.rs`)

- `mark`: `SAVEPOINT ivm_engine_mark` (a second mark releases and reopens it). From then on the
  connection is inside one transaction; each settle's `SAVEPOINT ivm_engine_frontier` nests
  inside it and its `RELEASE` no longer commits.
- `rewind`: `ROLLBACK TO ivm_engine_mark`. SQLite copies the journaled page images back:
  sources, `_i` images, DRed tables, `ivm_term` and the constructor tables all return to the
  mark at once, with no statement per relation. The savepoint stays open.
- The in-memory connection journals in memory; the extra cost is one page copy on the first
  write of each page after the mark (unit 1 `unit_us` did not move).

### dl8 (`src/_6_eval/_9b_ir_eval.rs`)

`EngineSession` caches `mapped` (term to cell), `decoded`, `answered`, `last_seeds` and
`initialized`. `mark` clones them into `Marked`; `rewind` restores them with the engine, since a
cell interned after the mark can be reissued for a different term. `IrSession::{rewinds, mark,
rewind}` forward to the session; a failed rewind drops the session like a failed apply.

### Macrotime session (`src/_1_macrotime/_4_expand.rs`)

1. `prepare` installs, settles the macro seeds into `base`, then, when `ir.rewinds()`, marks.
   `macrotime prepare` logs `retire="rewind"` or `retire="retract"`.
2. `enter`: on a rewinding engine with `dirty` set (a unit settled since the mark), rewind and
   clear `net`: the engine holds `base` exactly, so the rows above `base` are none. Then the
   unit's seeds settle at +1 only. `macrotime enter` logs `rewound=true`.
3. `leave` records `resident` only on an engine that does not rewind; ir:dd still retracts the
   previous unit's seeds at -1 inside the next unit's first settle, as on page 2.
4. `reprepare` prepares (and marks) again and sets `dirty` after settling the whole graph.

The unit reads `net` and `available` exactly as before; after a rewind `net` is this unit's
closure minus `base`, the same rows the retraction settle produced.

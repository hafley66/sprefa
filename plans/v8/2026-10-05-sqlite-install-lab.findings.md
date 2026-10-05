# Findings: lab-20261005-sqlite-install

Brief: `plans/v8/2026-10-05-sqlite-install-lab.brief.md`. sprefa `lab/sqlite-install` from main
`f158d8039`; sqlite_ivm clone `sqlite_ivm/` (branch `lab/sqlite-install` from main `a9e4b37`).
Debug dl8 (`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`); the install bench is
`--release`. `-j 4`, `--test-threads=1`. Load 2.7-9.3 (`uptime` around every timing; one other
lane on the machine; the first census started at 15.5).

## Harness

- `crates/ivm-sqlite/tests/12_install_bench.rs` (ignored tests):
  - `install_bench`: per corpus, interleaved runs after a warm-up: install ms, CREATEs executed
    (`SQLITE_TRACE_STMT`), statements run, CREATE execution time by schema position
    (`SQLITE_TRACE_PROFILE`), dd install of the same IR, cold prepare of every settle statement.
    Knobs `IVM_BENCH_RUNS`, `IVM_BENCH_PRAGMAS`, `IVM_BENCH_FILE`, `IVM_BENCH_SHARDS`,
    `IVM_BENCH_IMAGE=cold|disk`.
  - `create_scaling`: synthetic schema (3 objects per node), DDL cost per object against
    schema size and per variant.
  - `plan_text_dump`: settle statements + `sqlite_master` per corpus, for byte-identity checks.
- Corpora: `tests/corpus/8_c15_program.json` and `tests/corpus/install/{1_macro_library,
  2_macrotime,3_registry_comptime}.json`, dumped by `DL8_IR_DUMP=<dir>` (sprefa commit
  `e3014dc0d`) from `dl8 build std/tsi_registry_emit.dl7`. Re-dumped on `f158d8039`: byte-identical
  to the dump of the stopped attempt (IR unchanged by the CTE fix).
- A/B: one bench binary per arm, run round-robin, 3 rounds x 5 runs; numbers below are the
  median of the 3 round medians.

### Are the macro library (#1) and macrotime (#2) IRs identical across processes?

Same program: yes (same bytes, same fingerprint, every run). Different programs: no. Of
`1_settled.dl7`, `0_catalog.dl7` and the registry, the macro library IRs differ in 25 seed
source names (`__ir_seed_n<term id>_a<k>` and their derived relations) and 16 `Mfp` filter
literals (term ids); strata, outputs and texts are equal. The term ids depend on what the
program interned first. The node DDL of those IRs carries neither, which is what A4 keys on.

## A0: where install goes (base `a9e4b37`)

`sample` over the bench (all 4 corpora), self time under `Sqlite::install_on`:

| item | share |
|---|---|
| `OP_ParseSchema` after each CREATE: `SELECT*FROM sqlite_master WHERE tbl_name='x' AND type!='trigger'`, a full scan (no index on the schema table) + parse | 51.3 % |
| Rust plan compile and SQL text generation (`compile_ir`) | 18.8 % |
| `execute_batch` tail copy: `sqlite3Prepare` copies the remaining batch text when `zSql[nBytes-1] != 0`; the batch ends in the program JSON | 8.9 % |
| prepare of each DDL statement (parse, codegen) | 8.4 % |
| `sqlite3FkClearTriggerCache` on every `OP_SetCookie`: walks every table of the schema | 7.9 % |
| other DDL execution (btree create, cookie, inserts) | 4.2 % |
| terms tables, bootstrap, rest | 0.5 % |

The first two DDL items are O(objects in the schema) per CREATE, so install is O(objects²).
`create_scaling`, one schema, `execute_batch`: 26.0 / 39.2 / 70.0 / 124.6 us per object at
750 / 1500 / 3000 / 6000 objects. The bench shows the same per position: c15 CREATE execution
14 / 47 / 61 / 85 / 113 us per object by fifth of the batch.

## Arms

Install bench, ms (CREATEs executed in parentheses). dd installs the same IRs in 36.1 / 10.1 /
10.3 / 28.8 ms.

| arm | c15 | macro library | macrotime | registry comptime | settle delta (census) | suites (sqlite / dd) | kept |
|---|---|---|---|---|---|---|---|
| base `a9e4b37` | 247.6 (2477) | 54.9 (999) | 56.1 (1011) | 348.3 (2953) | - | 166/166 189 s / 166/166 115 s (coordinator) | - |
| A1a `sqlite3_exec` for the install batch (own run: base 234.8 / 51.9 / 52.1 / 331.4) | 216.4 | 48.5 | 49.7 | 298.6 | not separate | with the set | yes |
| A1 pragmas (`journal_mode=OFF`, `synchronous=OFF`, `temp_store=MEMORY`, `cache_size`), in-memory, on A1a | 219.4 vs 222.8 | 49.2 vs 50.3 | 49.8 vs 51.0 | 306.2 vs 309.1 | - | - | no |
| A1 file database (no pragmas / pragmas), on A1a | 255.0 / 236.7 | 70.5 / 55.6 | 75.3 / 56.0 | 350.8 / 323.7 | - | - | no |
| A1b 8 attached shard schemas, on A1a | 111.8 | 33.3 | 33.6 | 142.8 | see set | with the set | yes |
| A1b shards 4 / 10 | 120.1 / 106.0 | 33.8 / 31.6 | 34.3 / 31.9 | 162.0 / 134.7 | - | - | 8 chosen |
| A4 alone (1 shard): miss / process hit / disk hit | 215.6 / 65.8 / 66.4 (2477 / 219 / 219) | 50.5 / 18.5 / 19.9 | 51.2 / 19.1 / 20.3 | 309.6 / 79.0 / 82.6 (2953 / 178 / 178) | - | - | as part of set |
| A1a + A1b + A4: miss / process hit / disk hit | 114.0 / 66.6 / 69.2 (2477 / 219 / 219) | 34.1 / 19.3 / 20.3 (999 / 77 / 77) | 34.7 / 19.5 / 20.5 (1011 / 83 / 83) | 147.9 / 79.2 / 82.1 (2953 / 178 / 178) | -2 % | 166/166 154 s / 166/166 103 s | yes |
| A2 fewer objects | not built | | | | | | - |
| A3 shared node-keyed tables | not built | | | | | | - |
| A5 lazy objects | not built | | | | | | - |
| A6 statement reuse | no change | | | | | | - |

Pragmas: within 1-2 % on an in-memory database, and `journal_mode=OFF` removes the journal that
`rewind` (`ROLLBACK TO ivm_engine_mark`) needs. Nothing kept from them.

### Census (104 programs, `dl8 compile`, ir:sqlite, sums, s; interleaved per program)

| item | base | A1a+A1b+A4, no cache dir | + `IVM_SQLITE_IMAGE_CACHE`, empty at start | + cache dir warm |
|---|---|---|---|---|
| `ir install install_ns` | 23.32 | 13.92 | 10.96 (89 of 315 installs miss) | 8.75 (0 miss; instrumented binary) |
| macrotime phase | 24.41 | 20.14 | 17.49 | 18.36 (instrumented) |
| comptime phase | 31.64 | 25.80 | 25.30 | 24.19 (instrumented) |
| macrotime + comptime − install (settle and the rest) | 32.73 | 32.02 | 31.83 | 33.80 (instrumented) |
| wall of the 104 runs | 69.4 | 59.4 | 56.1 | 55.9 |

All 104 programs exit 0 in every column. With a warm directory the macro library and macrotime
shard images hit for every program (their node DDL is shared); the comptime image hits when the
same program compiles again.

### Audit ledger rows 1-4 (`2026-10-05-sqlite-audit.findings.md`), census sums, s

Own counters (scratch patch, not committed; applied to each tree, built, reverted): first sight of
a statement text on a connection is prepared alone and timed. Row 1: statements carrying
`MATERIALIZED (` CTEs. Row 3: statements naming the bundle output's `_i` or `_d`; exec covers the
settle fills, integrates and `output_delta`, not the `weight_delta` read. Row 2: the install DDL
batch (and the shard batch or image load). Row 4: `compile_ir`.

| row | audit (1074daa, its counters) | 1074daa | a9e4b37 (lab base) | lab, no cache dir | lab, warm cache dir |
|---|---|---|---|---|---|
| 1 first prepare of CTE fills | 21.47 | 23.07 | 7.30 | 7.41 | 7.40 |
| 2 install DDL batch | 15.77 | 14.84 | 16.83 | 7.18 | 2.00 |
| 3 bundle output path: prepare / exec | 9.47 / 5.75 | 9.94 / 18.19 | 1.84 / 4.27 | 1.86 / 4.28 | 1.86 / 4.28 |
| 4 SQL generation | 6.58 | 6.18 | 5.95 | 5.98 | 5.94 |
| install total (`install_ns`) | 23.05 | 21.53 | 23.30 | 13.96 | 8.75 |

Rows 1 and 3 moved with `a9e4b37` (the CTE cap and stored unions); this lab leaves them within
2 %. Row 2 grew 13 % with `a9e4b37` (more stored deltas: c15 pins 2477 CREATEs) and drops 57 %
with A1, 88 % with a warm image directory. Row 4 is unchanged and is now the largest install
item: with an image hit, `compile_ir` is 74 % of the install self time (sample), shard image load
12 %, the main-schema batch 7 %.

### Registry build (`dl8 build --out <dir> std/tsi_registry_emit.dl7`), ir:sqlite, 5 interleaved rounds

| build | real s (sorted) | max RSS MB |
|---|---|---|
| base | 1.43 1.44 1.45 1.52 1.82 | 184-203 |
| A1a+A1b+A4 | 1.17 1.20 1.22 1.22 1.40 | 199-214 |
| A1a+A1b+A4, warm cache dir | 1.10 1.10 1.11 1.12 1.15 | 241-255 |

Output `cmp`-identical to `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` on every run,
both engines. RSS: the process layer keeps each image (registry comptime 13.6 MB, macro library
4.4 MB) and a deserialized schema is a resizable `memdb` buffer.

## Arms not built, with the data

- A2 fewer objects. Registry comptime objects: 1727 `_d`, 872 `_i`, 112 `_x`, 15 `_g`, 40 SCC
  scratch (all in shards), 143 host sources of which 112 seed sources (main). At the end of the
  registry build 420 of 872 comptime `_i` hold no row (macro library 296 of 332, macrotime 253 of
  335; fixture comptime 373 of 410; `_d` are cleared every settle, so not counted). Every
  removable kind in `2026-10-03-sqlite-creates.findings.md` other than the seed sources is a
  settle-plan change (Mint deltas, SCC copies, threshold images), outside this lane's scope.
  Under A4 a shard object costs nothing on a hit; on a miss about 25 us (registry, 8 shards).
  The 112 seed sources live in main and run on every install: 178 main CREATEs remain on a
  registry hit, part of the 7 % main batch.
- A3 shared node-keyed tables. Candidates: no crate provides it; it is a schema form over
  rusqlite. Micro (python sqlite 3.53.4, 1000 nodes x 300 rows, insert 5 delta rows + join to
  the next node's image + clear, per node): per-node tables DDL 108-110 ms, settle 14.5 ms;
  shared `(node, ...)` tables DDL 0.1 ms, settle 10.2 ms (the shared form also reuses one
  statement text; the plan's statements have distinct shapes, so that part does not carry over).
  Install upside over A1a+A1b+A4: at most the non-generation remainder of a hit (about 20 ms
  registry) and the miss DDL (about 75 ms registry). It rewrites every settle statement
  (`3_nodes.rs`), so not attempted here.
- A5 lazy objects. A CREATE changes the schema cookie of its schema; every prepared statement
  that uses that schema re-prepares on its next step. First prepares already cost 7.4 s of the
  census (row 1); creating tables during settle would add re-prepares there. With A4 the eager
  objects of a hit cost an image load (12 % of a hit install). Not attempted.
- A6 statement reuse. Cold prepare of all settle statements (bench): base 181-187 / 56-58 /
  56-59 / 209-216 ms; A1a+A1b+A4 189-198 / 58-61 / 59-62 / 217-233; same binary with 0 shards
  185-188 / 56-57 / 57 / 214-220. Shards add 2-4 % to a cold prepare (unqualified names resolve
  across 10 schemas; a statement opens each schema it touches); census row 1 +1.5 %. The
  statement cache is already sized to every statement (`4_engine.rs`), and the audit saw no
  evictions. Nothing changed. Qualifying table names with their shard would remove the lookup
  across schemas but changes settle statement text.
- Rust generation (row 4), tried: `with_inline_deltas` visiting only reached definitions,
  byte-identical plans (`plan_text_dump`), measured 0-2 %; reverted.

## Recommendation

Merge sqlite_ivm `lab/sqlite-install` (A0 harness, A1a, A1b, A4), and have dl8 set
`IVM_SQLITE_IMAGE_CACHE` (a directory under the user cache or the target dir) so macro library
and macrotime images hit across programs. Install then costs 13.9 s of the census without the
directory and 8.75 s with a warm one, against 23.3 s at base. Next lever on install: SQL
generation (row 4, 5.9 s), now 74 % of a hit.

## How it works

### A1a: `sqlite3_exec`

rusqlite's `execute_batch` prepares each statement from a `&str` slice and passes its byte length.
SQLite copies the SQL before parsing whenever the byte before that length is not NUL, which is
always true for a slice of a longer text, so each statement of the install batch copied the rest
of the batch, program JSON included. `sqlite3_exec` takes the NUL-terminated text and prepares
in place. Same statements, same order.

### A1b: shard schemas

SQLite keeps one schema table per attached database and, after every CREATE, reparses the new
entry by scanning that table and clears the foreign-key trigger cache over every table of that
schema. `Engine::install` attaches 8 in-memory databases `ivm_s0..ivm_s7` and places each node
table (and its indexes) in one of them round-robin, so each scan covers about an eighth of the
objects. The tables the two output views read stay in `main`, because a view only reads its own
database. SQLite resolves unqualified table names across attached databases, so no settle
statement changes. A savepoint spans every attached database, so `mark`/`rewind` and the
per-settle savepoint behave as before. `install_on` keeps one schema; `install_on_with` takes the
shard count.

### A4: image cache

A node table's DDL names it by node index and gives its column count and keys; it carries no
relation name, literal or row. So the shard DDL text of an install is a function of the plan's
shape, and the same text always produces the same empty schemas. On the first install of a text
the shard DDL runs as its own batch ahead of the install savepoint, and each shard is
`sqlite3_serialize`d; later installs `sqlite3_deserialize` the bytes into the attached schemas
instead (a schema inside a transaction cannot be deserialized; a failed install drops the
fresh connection). The key hashes the SQLite version, the shard count and the DDL text; the
text is stored with the image and compared on every hit. The process map is always on;
`IVM_SQLITE_IMAGE_CACHE` adds a directory (one file per key, written to a temp name and renamed).
Misses log `image cache miss` at `ivm_sqlite::image` info; unreadable or mismatched files warn and
run the DDL. Sources, catalog, terms, stage and the output tables still run in `main` on every
install. Feature `image` (default on) gates it; extension builds turn it off because the
loadable-extension bindings have no `sqlite3_serialize`.

## Validation (A1a+A1b+A4)

- Registry `cmp`: identical, ir:sqlite and ir:dd, and on all 5 timed sqlite builds per variant.
- Macrotime oracle: `dl8 expand` on the 24 `oracle/macrotime/*_<n>.json`, both engines, base vs
  lab stdout+stderr+exit: 48 pairs, 0 differences.
- Suites: `DL8_ENGINE=<e> cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book
  --test-threads=1`, extension dylib rebuilt from the clone: ir:sqlite 166 passed 0 failed 154 s
  (load 4.8); ir:dd 166 passed 0 failed 103 s (load 4.7).
- sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1`: 245 passed,
  0 failed. `11_settle_spans` pin 107 -> 108 statement spans (the shard batch);
  `6_install_growth` pins unchanged (2477 CREATEs).
- `plan_text_dump`: settle statements and schema unchanged by the reverted generation trial.

## Overlap

No settle, closure-pass or planner code changed (`3_nodes.rs` untouched). Changed: `catalog.rs`
(install batch execution, shard placement of DDL), `4_engine.rs` (`install_on_with`, ATTACH,
CREATE-count log over every schema), `meter.rs`, new `1_image.rs`, crate features, `frontier-ext`
lab manifest (`default-features = false`).

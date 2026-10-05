# Findings: ir:sqlite cost ledger against ir:dd (audit)

Brief: `plans/v8/2026-10-05-sqlite-audit.brief.md`. sprefa `bc0439ed8`, sqlite_ivm `1074daa` (clone
inside the worktree). Debug binaries (`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`),
`RCARGO_OFF=1`, `-j 4`. Load 3.7-7.8 during the runs (another lane building).

## Method

- Untimed baselines: `/usr/bin/time -l`, 3 interleaved runs per engine.
- Stacks: `samply record -r 4000 --unstable-presymbolicate` on the registry build, both engines;
  main-thread wall per sample, rolled up by function.
- Temporary counters (scratch only, reverted, `git status` clean in both trees afterwards): a
  `prof` module in the clone's `ivm-engine`, timers around every settle statement in
  `NodesPlan::exec` (prepare timed apart from execute; first sight of a SQL text on a connection
  = first prepare), every step of `Sqlite::settle`, `settle_inner`, `install_on`,
  `install_program`, `NodesPlan::compile`, the term functions, and the dl8 wrappers in
  `_9b_ir_eval.rs` (same wrappers on ir:dd). Phase from `dl8::phase`. A second census run also
  logged every first-prepared statement (bytes, CTE prefix bytes, CTE names, connection); its
  file writes inflate wall, so totals come from the run without it.
- Census: the 122 `.dl7` under `fixtures/ examples/ book/src/probes/ std/`, `dl8 compile`,
  `RUST_LOG=dl8::phase=info,dl8::ir=info`; 104 exit 0 on both engines; sums over those 104.
- c15: `dl8 expand oracle/macrotime/15_standard_plus_1.json` (macro library install + macrotime
  install). The `ivm-sqlite` install corpus `tests/corpus/8_c15_program.json` was not run.

## Totals

| workload | ir:sqlite | ir:dd | note |
|---|---|---|---|
| census, phase sum (instrumented) | 89.49 s | 15.83 s | 104 programs, 315 installs, 506 settles per engine |
| census, phases sqlite / dd | read 1.41 / 1.20, macrotime 32.34 / 5.82, lower 0.52 / 0.47, check 0.17 / 0.12, comptime 54.78 / 7.95 s | | first uninstrumented-load run: 84.97 / 17.77 s |
| registry `dl8 build std/tsi_registry_emit.dl7`, real, 3 runs | 1.74, 1.66, 1.67 s | 0.35, 0.34, 0.35 s | `cmp` identical every run; RSS 241-268 / 110-124 MB |
| c15 `dl8 expand`, real, 3 runs | 0.32, 0.28, 0.33 s | 0.08, 0.07, 0.07 s | JSON identical across engines |

Per phase, census (sqlite / dd, s):

| phase | install | settle | statement first-prepare | statement exec | DDL | SQL generation | term interning |
|---|---|---|---|---|---|---|---|
| macro library comptime (identical every process) | 6.12 / 0.66 | 6.38 / 0.75 | 4.56 | 1.30 | 4.27 | 1.62 | 0.37 / 0.05 |
| macrotime | 6.11 / 0.61 | 9.75 / 1.86 | 4.98 | 3.64 | 4.26 | 1.65 | 1.05 / 0.14 |
| comptime | 10.83 / 0.94 | 36.41 / 1.92 | 26.33 | 8.60 | 7.24 | 3.31 | 1.82 / 0.49 |

## Ledger (census, sorted by sqlite ms)

dd column: the dd cost of the same work. Rows 1, 3 (prepare part), 5, 11 have no dd counterpart
(dd compiles no statements; its operators are built inside install). Settle rows fall inside dd's
4,533 ms settle; install rows inside dd's 2,213 ms install (dd does not split further).

| # | item | where | sqlite ms | dd equivalent ms | per | cause | candidate fix | owned by | prior |
|---|---|---|---|---|---|---|---|---|---|
| 1 | first prepare of settle fills that carry inline-delta CTEs | `3_nodes.rs:1489-1520` (`with_inline_deltas`), `:883-888` (join terms), `:1966` | 21,469 | 0 | install (first execution of each statement) | 27,476 statements, 105.8 MB. Prepare is superlinear in CTE nesting: statements with 0-2 CTEs 42 ns/byte, 6-10 CTEs 142, 11-20 CTEs 243; statements with 6+ CTEs (14,847) hold 28.5 of 33.4 s of CTE-statement prepare (logging run, 122 programs). Each join delta references each input delta twice (`Δa⋈Δb`, `I⋈Δb`, `Δa⋈I`); SQLite copies a CTE body at every reference (`selectExpander` 184 of 300 ms `sqlite3SelectPrep`, alloc/`Dup` 213 of 584 ms prepare, registry profile). Micro (python sqlite 3.53.4): chain of 12 CTEs referencing the previous once 0.14 ms, twice 18 ms. 37% of CTE bytes (59 MB) repeat a definition already present in another statement of the same install | bound inline chains by reference-weighted expansion (store the `_d` past a bound) in `place` (`:497`); join delta form that references each input delta once; one stored `_d` for a delta read by several statements | new (plan shape); install lab A6 measures prepare | ir-speed 10-02 #7 "cold statement preparation about 0.4 s" (known, unsized); mechanism new |
| 2 | install DDL batch | `catalog.rs:342-368`, objects `3_nodes.rs:1603-1700` | 15,771 | inside 2,213 | install | 328,483 CREATE in 315 installs. Fit over 354 installs: `ns = 16,900·c + 28.8·c²`, the quadratic term is 65% of DDL time. After each CREATE SQLite re-reads the new entry by scanning `sqlite_schema` on `tbl_name` (`OP_ParseSchema` → `sqlite3_exec`, 200 of 367 ms DDL in the registry profile) and clears the FK trigger cache over every table (36 ms). Micro: 500 / 1000 / 2000 / 4000 CREATE = 21 / 29 / 49 / 87 µs each | installed-image cache (micro: `deserialize` of a 4000-table image 5.9 ms vs 362 ms of CREATE); fewer objects (`2026-10-03-sqlite-creates`); objects split over ATTACHed schemas (micro: 4000 CREATE in 1 schema 362 ms, 4 schemas 121 ms, 8 schemas 90 ms) | install lab (A1, A2, A4) | ir-speed 10-02 #2 and line 69 (known, "verify" in lab brief); in flight |
| 3 | bundle output path: one inline Union CTE read by 3 statements | `4_engine.rs:197-226` (bundle), `3_nodes.rs:815` (union inline), `:523` (kept inline: gain < 2), `:1775-1787` (output delta), `:1683-1687` (bundle `_i` upsert, delete) | 15,227 | prepare 0; exec inside 4,533 | prepare per install, exec per settle pass | output delta and both bundle `_i` statements each embed every output's delta chain: 102 KB of SQL on average, up to 236 KB and 684 CTEs (registry). Prepare 9,473 (integrate 6,618 + output 2,855), exec 5,754 (integrate 3,685 + output 2,069). The delete returns 0 rows on 499 of 506 executions (1,657 ms). Integrate split scaled from the logging run (91% of integrate+CTE prepare, 78% of its exec) | store the bundle `_d` once per settle; delete `w = 0` rows without re-reading the delta (partial index `WHERE w = 0`, or the upsert's touched keys) | new | materialize-less 10-02 lists `_i` on inline nodes recomputing the chain; bundle size new |
| 4 | SQL generation (Rust) at install | `3_nodes.rs:1546-1815`, `:1890` (`inline_statements`), `:1916` (`prefix`), `:1817` (`index_delta_dependencies`) | 6,581 | inside 2,213 | install | 620 MB of settle SQL generated over 315 installs (2.0 MB per install); 294 MB ever prepared. Lowering 1,601, assembly 1,338, CTE splicing 1,734, `replace("ivm_n", prefix)` over every statement plus dependency scans 1,782 | generate statement text on first execution; prefix at generation; skip text for statements gated off | install lab | new (sized) |
| 5 | DRed loop statements, first prepare | `3_nodes.rs:1622-1734`, `:2015-2098` | 4,131 | 0 | install | 155,682 of 213,043 first-prepared statements; 11 statements per LetRec variable plus every loop fill twice (delete and insert phase); 76 MB | fewer distinct loop statements (one parameterized statement per variable kind) | comptime-settle lane | new |
| 6 | settle fills carrying CTEs, execution | `3_nodes.rs:2125-2147`, `:1500-1507` (work prefix) | 3,371 | inside 4,533 | settle pass | 39,091 executions, 30,676 return 0 rows (2,419 ms). The gate (`:2133`) sees only physical deltas; the counting prefix `(SELECT frontier_<p>_work(slot, count(*)) FROM <cte>) CROSS JOIN` materializes each counted join CTE even when the body reads none of it | drop the counting prefix when counters are unmeasured; gate on inline-chain inputs | comptime-settle lane | materialize-less 10-02 notes the prefix (known) |
| 7 | term interning through SQL | `2_terms.rs:351-382`, `_9b_ir_eval.rs:700-886` | 3,238 | 673 | term | 438,923 constructor inserts, each a lookup miss, `INSERT INTO ivm_term … SELECT coalesce(max(id),0)+1 … RETURNING id`, and a constructor insert, with lookup and insert SQL `format!`ed per call (2,037 ms inserts, 166 ms lookups); 80,562 `intern_text` calls because `bootstrap_changes` re-interns every lowered text each call (`_9b_ir_eval.rs:808-811`) | ids from a cached max in Rust, one multi-row insert per constructor, text cells kept in the session | new | ir-speed 10-02 added `intern_terms` (fixed settles; per-term cost new) |
| 8 | DRed loop execution | `3_nodes.rs:2015-2098` | 2,057 | inside 4,533 | round | 369,506 executions, 295,474 return 0 rows; 870 fixpoints, 895 delete rounds, 1,669 insert rounds; stash/seed/rederive/restore/finish run per variable on every fixpoint | skip stash/restore and rederive when the delete phase found nothing | comptime-settle lane | cheap-unit-retraction 10-05 measured the phases (known) |
| 9 | per-row and per-settle work outside the plan | `engine.rs:148-263`, `4_engine.rs:341-547` | 1,985 | inside 4,533 | row / settle | stage 616 (958,795 rows into `frontier_<p>_stage`), host-table writes 342 (one statement per row), node-delta insert 143 + `read_delta` 190 (rows written to `frontier_<p>_delta` and read back, then discarded for bundles, which read `weight_delta`), `weight_delta` read and decode 200, savepoint release 47, bump 20, validate 16, `count(*) FROM ivm_term` 13 (2,069 runs), consolidate 5, clears 8; the other 384 ms is per-row Rust: each change becomes a `SqlChange` with a cloned relation name (`4_engine.rs:420-428`), cloned again per output (`:432`), and the stage parameters clone the name a third time (`engine.rs:191-197`) | return `NodesPlan::run` rows for bundles (drops 533 ms of delta-table round trip); stage each row once | comptime-settle lane | ir-speed 10-02 made the delta insert one prepared statement (partly fixed) |
| 10 | integrate statements of inline non-bundle nodes carrying CTEs | `3_nodes.rs:1683-1687`, `:1666` | 1,680 | inside 4,533 | settle pass | an inline node that is integrated re-evaluates its CTE chain in both the upsert and the delete; prepare 623, exec 1,057; most deletes return 0 rows | as row 3 (delete by `w = 0`); store `_d` for integrated inline nodes | new | materialize-less 10-02 (known) |
| 11 | Rust in `NodesPlan::run` outside statements | `3_nodes.rs:2111-2176`, `sqlite_ext` statement spans | 1,049 | 0 | statement | gate scans, a disabled `stmt` span per statement, counter instrumentation share; uninstrumented registry profile: 22 of 1,984 ms | none listed | — | new |
| 12 | engine drop | `4_engine.rs:11-21` (connection owner), dropped from `_9b_ir_eval.rs:330`, `:442` | 805 | 1 | install | closing the connection finalizes every cached statement (cache sized to all statements, `4_engine.rs:313-316`) and frees the schema | reuse one connection across installs, or skip the close at process exit | new | dd-speed 10-03 fixed the dd side (detached worker) |
| 13 | integrate statements without CTEs | `3_nodes.rs:1683-1687` | 710 | inside 4,533 | settle pass | 6,460 executions, 3,184 deletes return 0 rows | delete by `w = 0` | new | new |
| 14 | install remainder | `2_terms.rs:54-87`, `catalog.rs:304-402`, `:634-665`, `4_engine.rs:230-332` | 697 | inside 2,213 | install | dictionary/constructor tables 250, bootstrap source check 65, catalog reads (2 `PRAGMA table_info` + `CREATE IF NOT EXISTS`) 49, program JSON 46 (27.5 MB), release 39, schema read + source DDL text 24, scan/build 20, bundle IR 20, engine struct 19, `open` 17, DDL text 16; 132 not split | catalog DDL once; skip program JSON for in-memory engines | install lab | creates 10-03 #13 (catalog DDL twice, known) |
| 15 | settle fills without CTEs (prepare 172, exec 315) | `3_nodes.rs:2125-2147` | 487 | prepare 0 | settle pass | 9,799 executions | — | — | — |
| 16 | `count_work` counters | `3_nodes.rs:1990-2010`, called `:2148`, `:2035`; `ivm-engine/src/1_rel.rs:94` | 329 | 0 | settle / round | 14,368 `SELECT count(*) … GROUP BY … HAVING` for counters; `Sqlite` does not override `install_unmeasured`, so every settle counts | skip in unmeasured mode | comptime-settle lane | ir-speed 10-02 #6 (SQL cached: partly fixed) |
| 17 | stage scan, clears | `3_nodes.rs:2116-2119`, `:2170-2174` | 46 | — | settle | 551 `SELECT DISTINCT __table`, 6,546 clears | — | — | — |
| 18 | mark / rewind | `4_engine.rs:556-572` | 7 | — | unit | 104 marks, 40 rewinds | — | — | fixed (cheap-unit-retraction 10-05) |
| 19 | snapshot + decode | `_9b_ir_eval.rs:489-698`, `2_terms.rs:410-417`, `4_engine.rs:574-605` | 596 | 528 | snapshot | 7,146 `SELECT * FROM <ctor> ORDER BY c0` (694,393 rows) re-read per decode; 18,600 member snapshots (14,516 empty) | decode from rows minted since the last read | new | ir-speed 10-02 #8 fixed member snapshot (fixed) |
| 20 | read, lower, check phases; IR lowering; fingerprint | `src/lib.rs`, `_9a_ir_lower.rs`, `_9b_ir_eval.rs:473-481` | 2,866 | 2,507 | program / round | same Rust on both engines | — | — | — |
| 21 | other Rust round work (not instrumented) | macrotime expand, comptime rounds | 6,390 | 5,371 | — | remainder of the phase sum after rows 1-20 | — | — | — |
| | total | | 89,492 | 15,826 | | | | | |

Rows 19-21 are work both engines run; listed last. Inside rows 1, 3, 6, 8: the extra constructor-closure pass, 45 passes, 143 ms
(`4_engine.rs:440-504`; plain-inner 10-05 receipt; in flight on the comptime-settle lane).

### Sum of rows against measured total

- Rows 1-20 (instrumented): 83,102 ms of the 89,492 ms phase sum (92.9%). Row 21 is the remainder,
  6,390 ms; ir:dd's remainder is 5,371 ms on the same code. Unexplained ir:sqlite-only remainder:
  1,019 ms (counter formatting per executed statement and allocator traffic are not split out).
- Settle: rows 1, 3, 5, 6, 8, 9, 10, 11, 13, 15, 16, 17 = 52,541 ms = measured `Sqlite::settle`
  (rows 3 and 10 split integrate-with-CTE time by the logging run's bundle share).
- Install: rows 2, 4, 14 = 23,049 ms = measured `Sqlite::install`.
- Gap to dd: 73,666 ms. ir:sqlite-only work with no dd counterpart: statement first-prepare
  35,874 ms (all rows), DDL 15,771, SQL generation 6,581 (58,226 together). Statement execution
  13,543 and per-row/settle work 1,985 against dd's 4,533 ms settle; interning 3,238 against 673;
  drop 805 against 1.

## Counts (census, ir:sqlite)

| count | value |
|---|---|
| statements first-prepared | 213,043 (+315 output deltas); re-prepared (cache hit) 212,197, 90 ms; no evictions seen |
| prepared SQL | 294 MB; CTE prefix 164 MB (logging run, 104 programs); 69,230 statements carry CTEs and take 93% of prepare |
| statements executed | 459,426; zero-row 339,730 (74%) costing 5,881 ms of 13,543 ms |
| statements gated off (not executed) | 1,240,174 (step 147,842, integrate 167,116, clear 110,705, loop fills 281,646, loop integrates 531,968, SCC 897) |
| passes / rounds | 506 first passes, 45 extra closure passes; 870 fixpoints; 895 delete rounds, 1,669 insert rounds |
| DDL | 328,483 CREATE in 315 installs (331,736 statements in the batches), 1,043 per install |
| pragmas | 2 `PRAGMA table_info(frontier_catalog)` per install (630); none per settle |
| savepoints | per settle 1 `ivm_engine_frontier` (506) + 1 `frontier_sp_settle` per pass (551); 1 per install (315); 104 marks, 40 rewinds |
| JSON / text | program JSON at install 27.5 MB, 46 ms; `structural_fingerprint` serializes the IR (68 ms sqlite, 67 ms dd) |
| decode reads | 7,146 constructor-table scans (694,393 rows, 80 ms); 18,600 member snapshots (753,183 rows, 110 ms); `text` 99 calls |

Rows written per table kind (executions / ms / rows / zero-row executions):

| statement | n | ms | rows | zero |
|---|---|---|---|---|
| `INSERT _d` (fills) | 187,879 | 4,581 | 1,757,585 | 157,952 |
| `UPSERT _i` (integrate) | 31,929 | 3,283 | 2,475,236 | 6,455 |
| `DELETE _i` behind a CTE (integrate) | 9,893 | 2,377 | 3,144 | 9,632 |
| output delta read | 551 | 2,069 | 475,991 | 45 |
| `DELETE _i` (integrate) | 22,036 | 428 | 5,130 | 21,579 |
| `count(*)` counters | 14,368 | 329 | 523,381 | 0 |
| `INSERT _nx` / `_acc` / `DELETE _nx` (DRed) | 26,172 each | 107 / 46 / 35 | 111,483 | 24,982 |
| constructor / text mint inserts | 4,169 | 64 | 18,341 | 1,538 |
| `INSERT _hold` / `DELETE _d` stash | 17,822 | 57 / 31 | 260,119 | 11,544 |
| `DELETE _d` (clears) | 47,757 | 54 | 1,757,585 | 17,808 |
| `INSERT ivm_term` after mint | 2,650 | 12 | 18,410 | 0 |
| stage rows (`engine.rs`) | 551 settles | 616 | 958,795 | — |
| host source rows (`4_engine.rs`) | 958,795 | 342 | 958,398 inserted, 397 deleted | — |
| delta-table rows (`engine.rs`) | 551 | 143 | 475,991 | — |
| constructor terms (`2_terms.rs`) | 438,923 | 2,037 | 438,923 | — |

## Registry build and c15 (instrumented, ms, sqlite / dd)

| item | registry | c15 |
|---|---|---|
| phase sum | 1,732 / 343 | — |
| install (3 / 2 installs) | 405 / 40 | 113 / 13 |
| DDL | 305 | 79 |
| SQL generation | 94 | 30 |
| settle | 1,083 / 158 | 153 / 33 |
| first prepare | 478 | 101 |
| statement execution | 505 | 36 |
| bundle path (prepare + exec) | 65 + 88 integrate, 30 + 51 output | 7 + 5 output |
| outside the plan (row 9) | 65 | 5 |
| term interning (dl8 wrappers) | 96 / 15 | 6 / 1 |
| drop | 16 / 0 | 6 / 0 |

## Cross-check with prior findings

| prior item | file | status here |
|---|---|---|
| six installs per registry build → three (one macrotime engine) | one-macrotime-engine 10-05 | fixed; 315 installs = 3 per program |
| retraction settle between units → savepoint rewind | cheap-unit-retraction 10-05 | fixed; rewind 6.4 ms over 40 rewinds (row 18) |
| `plain_inner` LetRec dropped when unread | plain-inner 10-05 | fixed |
| extra closure pass after any `ivm_term` growth | plain-inner 10-05, comptime-settle brief | in flight (comptime-settle); 143 ms census |
| per-CREATE cost grows with schema size | ir-speed 10-02 #2, install-lab brief | in flight (install lab A1); quadratic fit and mechanism here (row 2) |
| avoidable objects (2,232 CREATE census) | sqlite-creates 10-03 | in flight (install lab A2); row 2 |
| macro library compiled identically every process | phase atlas lever 2, install-lab A4 | in flight; macro library is 13.0 s sqlite vs 1.5 s dd of the census (install, settle, interning, drop) |
| cold statement preparation | ir-speed 10-02 #7, install-lab A6 | in flight for measurement; size and nesting cause new (rows 1, 3, 5) |
| `_i` on inline nodes recomputes the CTE chain | materialize-less 10-02 | not fixed (rows 3, 10) |
| inline-join counting prefix | materialize-less 10-02 | not fixed (row 6) |
| `count_work` built with `format!`, uncached | ir-speed 10-02 #6 | fixed (cached); counting still runs every settle (row 16) |
| mint registration gated, DRed skips empty deltas | ir-speed 10-02 | fixed; 1,240,174 statements gated off |
| member snapshot as one index range | ir-speed 10-02 #8 | fixed (110 ms) |
| `object_of` string search per execution | ir-speed 10-02 #10 | in row 11 (spans opened per statement) |
| catalog DDL issued twice | sqlite-creates 10-03 #13 | not fixed (row 14, under 1 ms) |
| bundle output path 100-236 KB statements read three times | — | new (row 3) |
| prepare exponential in nested CTE references | — | new (row 1) |
| SQL text generated at install 2.0 MB per install | — | new (row 4) |
| term interning three statements per term, texts re-interned per call | — | new (row 7) |
| connection close 2.6 ms per install | — | new (row 12) |
| node-delta round trip discarded for bundles | — | new (row 9) |

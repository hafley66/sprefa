# SQLite UDF graft verdict

Contract: `plans/2026-07-29-sqlite-udf-graft-lab-header.md`.

The lab command ran twice:

```text
swipl -q -l v6/prolog/labs/sqlite_udf/sqlite_udf_lab.pl -g go -g halt
```

Each run printed exactly `PASS inventory`, `PASS capture`, `PASS drivers`,
`PASS graft`, and `PASS conformance`, in that order. The conformance runner
was executed by both lab runs. The compile roundtrip script was not run in the
worktree because its documented G1 step writes
`v6/prolog/compile/dl_view/`, outside this arc's write fence. That remains an
explicit no-drift slot below.

## Q1. v5 inventory

`src/db.rs` has 16 `create_scalar_function` call sites and 14 distinct names.
The header's 16-name count has no matching source inventory. Repeated
registrations account for the difference. Function bodies are at
`src/db.rs:474-621`; writer registration is at `:344-369`; read-only
registration is at `:416-427`; DSL mapping is in `src/lower.rs:9-22` and
`:302-326`.

The 16 call sites are `:352 sprf_sym_intern`, `:419 sprf_sym_intern`,
`:481 regexp`, `:518 sprf_split`, `:567 sprf_lower`, `:569 sprf_upper`,
`:571 sprf_lcfirst`, `:573 sprf_ucfirst`, `:575 sprf_trim`, `:581 sprf_norm`,
`:584 sprf_strip_prefix`, `:589 sprf_strip_suffix`, `:599 sprf_sym`,
`:605 sprf_lines`, `:610 sprf_replace_re`, and `:1744 sprf_sym_intern`.

Usage counts are lexical `rg` counts of call-shaped spellings in
`examples/**/*.dl` and `.dl/**/*.dl`, not AST counts. Regex usage is `=~`.

| name | arity | semantics | bare SQLite 3.46.0 core | examples | `.dl` |
|---|---:|---|---|---:|---:|
| `regexp` | 2 | Cached regex match; NULL pattern/value gives NULL | absent | direct 0; `=~` 78 | direct 0; `=~` 13 |
| `sprf_split` | 3 | Nonempty separator, zero-based index, negative index from end, miss NULL | absent | `split(` 34 | 0 |
| `sprf_sym_intern` | 1 | Empty dense id 0; other text hashes, queues, and returns dense dictionary surrogate | absent | 0 | 0 |
| `sprf_lower` | 1 | Unicode Rust lowercase, NULL propagation | `lower` | 1 | 0 |
| `sprf_upper` | 1 | Unicode Rust uppercase, NULL propagation | `upper` | 0 | 0 |
| `sprf_lcfirst` | 1 | Unicode first-character lowercase | absent | 2 | 0 |
| `sprf_ucfirst` | 1 | Unicode first-character uppercase | absent | 0 | 0 |
| `sprf_trim` | 1 | Rust Unicode-aware trim, NULL propagation | `trim` | 7 | 0 |
| `sprf_norm` | 1 | ASCII alphanumerics only, lowercased | absent | 0 | 0 |
| `sprf_strip_prefix` | 2 | Remove present prefix, otherwise preserve input; NULL propagates | absent | 11 | 0 |
| `sprf_strip_suffix` | 2 | Remove present suffix, otherwise preserve input; NULL propagates | absent | 0 | 0 |
| `sprf_sym` | 1 | Pure `StringId::of(text).sqlite()` content hash | absent | `sym(` 1 | `sym(` 6 |
| `sprf_lines` | 1 | Empty 0, otherwise Rust `str::lines().count()` | absent | 3 | 7 |
| `sprf_replace_re` | 3 | Cached regex replace-all with `$1` references; NULL propagates | absent; native `replace` is literal | 38 | 21 |

Native literal `replace(` occurs 17 times in examples and 0 times in `.dl`.
The `~~` glob operator occurs 5 times in examples and 0 times in `.dl`.

The Rust capture opened bare bundled rusqlite and v5 `db::open` connections.
Both report SQLite `3.46.0`. Bare core has `lower`, `upper`, `trim`, and
`replace`, including two-argument `trim`, and has neither `regexp` nor
`sprf_split`. The v5 connection reports the custom function subset. Receipt:
`v6/prolog/labs/sqlite_udf/v5-capture.jsonl`, with 16 corpus rows and 224
value records.

## Q2. Driver reality

The current constructor is `open_db(url)` at
`v6/sprefa-store/js/src/engine/lib.ts:42-48`; it creates an
`@libsql/client` connection.

| candidate | empirical receipt | registration result | connection facts | consequence |
|---|---|---|---|---|
| `@libsql/client@0.17.4` | SQLite `3.45.1`; unknown UDF errors | `createFunction`, `create_function`, `function`, `registerFunction` all undefined | number `1` into TEXT gives `"1.0"`; bigint `1n` gives `"1"`; both give INTEGER storage in INTEGER affinity | Current TS seam cannot register UDFs. |
| `better-sqlite3@13.0.1` | native load succeeded | `.function()` returned `better:x` | synchronous native connection, unlike async `SqlRunner` | Registration exists; changing seam requires connection and implementation changes. |
| `sqlite3@5.1.7`, npm package for node-sqlite3 | binding failed under Node 24.15.0 darwin arm64 | no UDF execution | missing `compiled/24.15.0/darwin/arm64/node_sqlite3.node` | Named driver slot unresolved. |
| `sql.js@1.13.0` | WASM load succeeded | `create_function` returned `sqljs:x` | in-memory WASM database | Registration exists; persistence and async store seam differ. |
| Rust sidecar with rusqlite functions | bundled SQLite `3.46.0` | direct registration returned `sidecar:ok` | Rust can own the connection and function body | Registration exists; rows or projected results cross the process boundary. |

Receipts are `node-driver-probe.json`, the final sidecar record in
`v5-capture.jsonl`, and `libsql-probe.db` under the lab directory.

## Q3. Graft shapes

The parity corpus has 16 path and symbol rows. Pure string, intern, split, and
line UDF comparisons passed 16/16. Regex comparisons passed 15/15 after
excluding the Rust inline `(?s)` row, which JavaScript does not parse.

| class | SQL-native | UDF | TS deopt | emit-time |
|---|---|---|---|---|
| Pure string: lower, upper, trim, lcfirst, ucfirst, norm, strip prefix/suffix | Core lower, upper, trim exist. Rust parity: lower 15/16, upper 15/16, trim 16/16. First-character and strip expressions passed 16/16 or 15/16. `norm` has no core equivalent. Expressions can ride P1-P3 SQL. | Current libsql cannot register. Alternative working drivers can. Every executing connection needs the registry. | Rust-compatible implementations 16/16; one delta row and no full-table scan. Work is after SQL selection. | Constants use the Rust evaluator; dynamic arguments remain runtime work. |
| Regex: regexp, replace_re | Bare core has no regex. Native literal replace passed 4/4 on the literal subset. General regex needs a function or sidecar. | Sidecar and alternative working drivers can register; JS compatible subset 15/15. | Delta-only mapping preserves row scope; syntax and NULL behavior must match Rust. | Constant expressions can be evaluated; dynamic patterns remain runtime work. |
| Intern and sym: sprf_sym, sprf_sym_intern | No core expression. Pure hash is expressible only with a function. Dense interning queues text and allocates dictionary state. | Current libsql cannot register. Alternatives can register a pure hash; dense interning needs dictionary state and writer staging. | Captured oracle 16/16. `sprf_sym_intern` is not stateless. | Pure `sprf_sym` is constant-evaluable. Dense allocation needs an explicit phase. |
| Line splitting: sprf_lines, sprf_split | Newline-count SQL matched the corpus for lines. Rust trailing-newline behavior requires a named assertion. Core has no split scalar. | Current libsql cannot register; sidecar and alternatives can. | Both matched 16/16 on delta rows. | Both are constant-evaluable with result-type checks. |

`graft-check.json` records `ts_deopt.full_table_scan: false`, one delta row for
native and UDF checks, constant emit-time arguments, and true P1, P2, and P3
source receipts. Core SQL fuses where Rust semantics match. TS deopt is
delta-only. Emit-time applies only to constants.

## Q4. Expression-lift assertion set

### P1 arrival and boundary

1. Typed columns have explicit `INTEGER` or `TEXT` affinity in DDL;
   `__delta_*` and boundary projections preserve it.
2. Integer arrivals enter INTEGER columns as SQLite integer. Text `"1"` and
   numeric `1` remain distinct.
3. Bind checks use plain `1` and bigint `1n`; cell storage and log encoding are
   asserted separately.
4. Every expression has a declared result type, checked or cast before insert.
5. UDF NULL behavior matches Rust without statement abort.
6. Native expressions read only current `__delta_*` rows; query plans reject a
   full current-relation scan for delta-only rules.
7. Registration is checked on writer, reader, frontier, and sidecar
   connections before generated SQL executes.
8. Comparison and arithmetic use typed SQLite values, not rendered text.

### P2 frontier

1. Current, next, and frontier TEMP tables repeat relation column types.
2. Recursive expressions consume new frontier rows or delta joins.
3. Frontier retraction emits the prior typed output as the negative delta.
4. The `1` versus `"1"` and number versus bigint checks run after a drain tick.
5. Fixpoint keys use typed rows, not JSON stringification.

### P3 support reconciliation

1. Support counts key typed row values for add, decrement, collection, and
   reinsert.
2. Pure expressions are deterministic and retractable; removal uses the prior
   byte-equal result.
3. `sprf_sym` can satisfy the pure condition. `sprf_sym_intern` needs staged
   dictionary handling.
4. Missing UDF registration is detected before support SQL executes.
5. `:=` output does not change support identity.
6. `log_deltas_follow_arrival_order` and
   `shuffled_arrival_reorders_log_deltas` remain byte-identical.
7. `typed_int_without_literal_witness` remains green.
8. NULL or failed expressions roll back result and support update together.

Evidence for bigint and typed columns is in
`v6/prolog/compile/SCOREBOARD.md:70-71,176-183,228-245,388-399`. P1, P2,
and P3 shapes are declared in `v6/prolog/compile/emit_ts.pl:1-12`.

## Q5. `sym`, `sym_intern`, and `content_id()`

`sprf_sym(text)` is pure `StringId::of(text).sqlite()`. `sprf_sym_intern(text)`
computes that hash, queues nonempty text on the write connection, and returns
the dense `_sym_dict` surrogate. Empty text is dense id 0. The read-only
variant returns the pure hash and does not queue or allocate.

The types-lab `content_id(Type, Cols...)` ruling uses
`f(type, canonical content)` as semantic identity, independent of insertion
order. A dense integer intern mate is allowed for SQL keys and references,
while dense assignment is order-dependent and excluded from semantic hashing
and tick-log identity. See `plans/2026-07-28-types-as-rels-verdict.md:313-329`
and `:913-955`.

`sprf_sym` can participate in a content identity bind only with type salt and
canonicalization. `sprf_sym_intern` can provide storage mate for an existing
semantic identity. A graft is refused if it hashes a dense surrogate into a
semantic content id, uses insertion order as semantic identity, or puts dense
ids in a byte-stable semantic log. Parent content hashes consume canonical
child identity, never the child's dense mate.

## Named slots

| slot | status | evidence or condition |
|---|---|---|
| `UDF_COUNT` | open discrepancy | Header says 16 names; source has 14 distinct names and 16 call sites. |
| `LIBSQL_UDF_API` | unresolved in current seam | libsql has no registration method and rejects `udf_probe`. |
| `NODE_SQLITE3_ABI` | unresolved | sqlite3 binding did not load under Node 24.15.0 darwin arm64. |
| `ROUNDTRIP_NO_DRIFT` | not run under fence | roundtrip writes compile-owned `dl_view`. |
| `INTERN_SIDE_EFFECT` | constrained | Dense dictionary allocation needs explicit staging. |
| `CONTENT_ID_COMPATIBILITY` | conditional | Type salt and canonical content are required; dense mates remain storage-only. |

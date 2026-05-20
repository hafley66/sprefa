# v4-glossary

Comprehensive glossary covering the sprf DSL surface, the v4 internals, the
compilation / IR vocabulary, the OSS prior art the advisor rounds drew on,
and the memory / RSS terminology used in the redo-discussions.

Cross-reference: see `v4-language-vector.md`, `v4-system-architecture-audit.md`,
`v4-sql-rule-query-plan.md`, `v4-recursion-surface-gaps.md`,
`v4-retraction-fixpoint-plan.md`, `v4-cursor-value-where-bytes-plan.md`.

A rule is a FUNCTION. Read = call. Write = return / yield. Never "channel" or
"sink". See `feedback_rule_is_function_not_channel.md` in memory.

Banned words across the codebase and prose: provenance, substrate,
load-bearing, regime. Use source/origin, base layer, critical, mode.

---

## 1. The sprf DSL surface

| Term | Meaning |
|---|---|
| `.sprf` | sprefa source file extension |
| `rule(:name){body}` | declare a rule. A rule IS a function: body returns rows |
| `r?(args)` | read: call rule `r`, get back its rows |
| `r!()` | return / yield a row from inside a rule body (legacy spelling; "yield" in the v5 sketch) |
| `repo()` | bare rule that binds SLUG + ROOT terms per emitted cursor |
| `ast(:lang)` | rule constructor over tree-sitter parse trees, parameterised by language |
| `fs()` | filesystem rule (paths, globs) |
| `not(rule?(args))` | antijoin surface. Lowers to SQL NOT EXISTS |
| dot-access | `term.col` projection on a bound term |
| Callable | a value variant; a rule passed/stored as a value (`callable-value` plan) |
| Cons-calling-unification | list + kwarg + body share one Cons cell, root = implicit body |
| metavariable / capture | a `$Name` placeholder inside a tree-sitter pattern that binds matched syntax |
| `?-then-ref` | imperative rule-read: bind with `?` then re-use the same name = intra-row self-eq |
| `re` body with `${}` holes | literal regex with escaped metachars; no holes = raw regex passthrough |

## 2. Proposed-but-not-built operators (the 15 from the language-advisor round)

| Term | One-line |
|---|---|
| `@example` | inline worked-example block attached to a rule; runs as doctest + LSP hover |
| Bidirectional pattern braces | same brace form matches and constructs; polarity decides |
| Focused hole `?{}` | typed metavariable; LSP completes from live facts (Sketch / Idris) |
| `tac{}` tactic block | rule body as ordered search tactics + `why?(row)` trace (Lean/Coq) |
| Quotient rule `by=K?` | representative-per-equivalence-class (Egglog e-classes) |
| Bag-vs-set sigil `r#` / `r%` | keep multiplicities vs collapse to set |
| `r?@rev(SHA)` time-shifted read | read rows as of another commit (Datomic as-of) |
| Pattern functor + cata | bottom-up CST fold via per-shape clauses (recursion-schemes) |
| GraphQL-shaped projection | nested rowsets after a rule call |
| Negative-information binder `!X?` | bind the witness of absence (Mercury / ASP) |
| `assume(...) in ...` | speculative add/retract for one query, discarded after |
| Confidence `~p` | lattice value on rows, propagates through joins (Dyna / ProbLog) |
| Region binder `[A..B]?` | bind a CST byte-range as a `Region` value with `.text/.lang/.parent/.children` |
| `lens(:name)` | named reversible projection (Boomerang) |
| `pin(:caret)` | editor caret position as a fact rules can read (Eve / SelectionRange) |

## 3. Compilation / IR

| Term | Meaning |
|---|---|
| IR | intermediate representation. The shape between surface and execution |
| HIR | high-level IR: typed rule functions after parse + name resolution |
| Logical plan / RA tree | relational algebra tree (Scan/Filter/Project/Join/AntiJoin/Union/Recurse) |
| Physical plan / StepGraph | executable plan; each Step is a function over the store |
| Fuser | sprefa's current pass that concatenates SQL from rule body. `compile/fuser.rs` |
| `Liftable` | current sprefa IR-ish enum tagging op classification (Scan-like / JoinStep / AntiJoin) |
| `FusedKind` | enum of fuse outcomes: FullSql / StreamedRust / Unfused. The Unfused fallback is the bug |
| `BindingGraph` | per-rule column-flow graph used by fuser to decide JOIN unification |
| Magic-set / demand transformation | rewrite predicates so only tuples reachable from a query goal compute (CodeQL) |
| Projection pushdown | drop columns no descendant reads |
| Predicate pushdown | move filters as close to scans as possible |
| CSE | common subexpression elimination |
| Cost-based join reorder | reorder JOINs by cardinality estimate, smallest scan first |
| Stored vs derived | per-rule choice: materialise rows to a table vs stream through a WITH-CTE. Glean / CodeQL `cached` / Soufflé `.printsize` |
| Workload-driven index cover | compute the minimum set of `(rel, col)` indexes that serve every rule's bind shape (Soufflé) |
| Substrait | portable protobuf relational IR adopted by Spark, DuckDB, DataFusion, Velox |
| Volcano / Cascades | classic cost-based optimiser frameworks |

## 4. Recursion / evaluation

| Term | Meaning |
|---|---|
| Datalog | declarative rule language with recursive predicates and stratified negation |
| Stratification | partition rules into layers so each negation crosses a layer boundary |
| SCC | strongly connected component. A recursive cluster of mutually-dependent rules |
| Semi-naive evaluation | fixed-point iteration that only joins against the delta of each round |
| Δ-relation / delta predicate | the new tuples added in the last round of semi-naive |
| DRed | Delete-and-Rederive. Standard retraction algorithm for recursive Datalog |
| Differential dataflow | every datum is `(row, time, diff)`; deletion is a `-1` diff. Materialize's base |
| Arrangement | shared indexed state in differential dataflow, reused across operators |
| Frontier | the time boundary past which differential can compact older diffs away |
| Salsa | rust-analyzer's per-query memoisation with read-set invalidation |
| Read-set | the inputs a query actually touched; used to decide if it needs rerun |
| Support ledger | sprefa's record of which witnesses derived which sink row; used for retraction |
| Counting algorithm | DRed variant that stores reference counts instead of recomputing on delete |

## 5. OSS prior art (one-liners)

| Project | What it is |
|---|---|
| CodeQL / QL | Semmle/GitHub. Datalog-flavoured code analysis, RA IR, magic-set, predicate cache |
| Soufflé | Datalog compiler to C++. Auto-index selection, RAM IR, delta relations |
| Glean | Facebook. Angle queries on a typed column store; stored/derived predicates |
| LogicBlox / LogiQL | most aggressive historical Datalog→SQL compiler |
| Materialize | differential-dataflow-based SQL engine. Incremental views first-class |
| Differential Dataflow | the Rust library Materialize sits on top of |
| Salsa | rust-analyzer's memoised query graph |
| ast-grep | tree-sitter pattern matcher, YAML rules, no joins |
| semgrep | generic AST pattern + taint dataflow, no joins |
| tree-sitter | incremental GLR parser with error recovery, lossless CST |
| DuckDB | embedded vectorized column-store SQL. Buffer manager + spill |
| DataFusion | Rust columnar query engine, Arrow-native, LogicalPlan API |
| Polars | DataFrame + lazy streaming engine in Rust |
| ClickHouse | block-streaming OLAP column store |
| Postgres | row-store SQL with `work_mem` per-operator budget |
| RocksDB / LevelDB | LSM key-value stores |
| Tantivy / Lucene | search engines with immutable segments + background merge |
| ripgrep | streaming grep with per-thread reusable buffers |
| GNU sort | external sort via spill + k-way merge |
| LMDB | mmap'd single-writer copy-on-write B-tree |
| ScyllaDB / Cassandra | LSM-style wide-column distributed stores |
| fzf / fd / bat | small tools with bounded ring/window memory |
| ZetaSQL | Google's resolved-AST SQL analyzer; engine-portable |

## 6. Memory / RSS / IO

| Term | Meaning |
|---|---|
| RSS | resident set size. Physical memory pages held by the process |
| Working set | the subset of state actively read in the current time window |
| Paging | OS swaps cold pages to disk under memory pressure. The thing we want to avoid |
| mmap | map a file into the address space; reads served by the page cache on demand |
| Spill | overflow in-memory operator state to a temp file |
| External sort | sort-on-disk via N runs + merge pass |
| Replacement selection | extends average run length to ~2× buffer for nearly-sorted input |
| Buffer manager | DuckDB-style page accountant that decides what stays in RAM |
| `work_mem` | Postgres per-operator soft memory budget |
| LRU | least-recently-used eviction policy |
| LSM | log-structured merge tree. Memtable → flush → SST → compact |
| Memtable | LSM's in-memory write buffer with a hard size cap |
| Segment | Lucene/Tantivy's immutable on-disk index unit |
| Compaction | merge smaller segments / SSTables / arrangements into bigger ones |
| Morsel | a small batch of rows or files handed to one worker in a parallel scan |
| Vectorized execution | operate on column blocks (1k-65k rows) instead of one row at a time |
| Block / vector | the batch unit; ClickHouse uses 65 536, DuckDB 2 048 |
| Volcano model | tuple-at-a-time iterator pipeline (older shape) |
| Doorbell | the producer→consumer signal that says "drain me" |
| Push / pull / dam | three message-flow shapes. The dam = bounded channel between push and pull |
| mpsc | multi-producer, single-consumer channel |
| Arena | bump-allocator region freed wholesale at a known lifetime boundary |
| Rev arena | proposed: per-`(repo, oid)` arena holding tree-sitter trees + source bytes |
| `ParsedFile` | proposed: `(Arc<str> source, Tree tree, content_hash)` tuple living in a rev arena |
| `StripedLru` | LRU sharded by hash to reduce lock contention |
| Owned batch | `Cursor` rows allocated for one rule firing, freed at end of `next()` |
| Page cache | OS-level RAM cache of file pages |
| io_uring / NVMe / TCP TSO / SIMD | hardware/OS batching surfaces; each is a {queue, worker, doorbell} |
| time-to-first-diagnostic | LSP UX metric: keystroke → first squiggle |
| bytes-scanned-per-RSS-MiB | cold-scan health metric: throughput per unit of peak memory |

## 7. sprefa-internal vocabulary

| Term | Meaning / file |
|---|---|
| v4 | current generation of sprefa. Source under `v4/src/` |
| v5 | the redo proposed by the language-design advisor |
| effect_runtime | sibling crate hosting `FactStore`, `Cursor`, the saga-style op runtime |
| `FactStore<Cursor>` | trait in effect_runtime; impls are `MemFactStore` / `SqliteFactStore` |
| `Cursor` | sprefa's row type. `cursor_codec.rs` |
| `SprfStore` | `store.rs`: interner LRUs for strings / files / repos / revs / paths |
| Intern-id FK | every term is interned to an int; joins are on ints not strings |
| `RuntimeGraph` | `runtime_graph.rs`: live wiring of rules + tables + memo subscriptions |
| `Memo` | `memo.rs`: per-rule memoised rowset with dirty-set + cap |
| Dirty source | `dirty_source.rs`: which files changed and need rerun |
| Sweep | the pass that drains the dirty set and re-evaluates affected rules |
| `SourceIndex` | `source_index.rs`: file-id table, immortal per repo |
| `SupportLedger` | tracks which witnesses derived which sink row; used by retraction / DRed |
| Stratify | `stratify.rs`: Tarjan SCC + negation-edge check; emits strata |
| `eval_stratum` | the semi-naive fixed-point driver in `fixpoint.rs` |
| `fuse_full_sql` | the current SQL-string emitter inside `compile/fuser.rs` |
| `fuse_streamed_rust` | unused in-memory path at `compile/fuser.rs:578` |
| `_strings` / `_files` / `_paths` | intern tables in SQLite |
| `_memo` | on-disk memo backing table |
| `_facts` | per-rule stored fact tables (for stored sinks) |
| `pending_core` | per-thread batched-write staging map in `store.rs:106`. Day-1 delete candidate |
| `dummy_batch` | vestigial shim in `fixpoint.rs:55-57`; recursion does not run on a batch of cursors |
| LSP | Language Server Protocol; sprefa's tower-lsp shim is in `lsp.rs` |
| Tier-0 audit | the most recent v4 correctness audit (avoid re-reading; plans/2026-05-19-v4-worst-audit.md) |
| `linux.sprf` | bench fixture under `v4/bench/`; sprefa run against the Linux kernel source |

## 8. Cross-report convergence

| Theme | All three redo-advisors agree |
|---|---|
| `compile/fuser.rs` is the wrong shape | lang-Chris kills it for a dataflow IR; sql-Chris splits it `plan()` + `emit_sqlite()`; rss-Chris forces a doorbell the fuser fights |
| Stored vs derived is a real distinction | sql-Chris promotes it to surface, lang-Chris assumes the WITH-CTE composition |
| SQLite stays | none of them rip it out on day one. DataFusion is the contingency if benches demand it |
| Tree-sitter trees are the working set | rss-Chris bounds them by rev arena + morsel; lang-Chris makes `Region` first-class so they are addressable |
| `Cursor` row + intern-FK model is right | not on anyone's hit list |

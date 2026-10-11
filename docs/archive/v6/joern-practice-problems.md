# Joern practice problems on the taint-walk corpus

Five problems for learning the Joern query language (the Scala DSL you type in a
Joern shell) against one small Rust corpus: `v6/tsv2/goldens/cpg_taint_walk_golden/corpus/`.
Each problem names the Joern construct it teaches, a query to type, the answer
this corpus should produce, and the sprefa dl6 machinery that covers the same
question (or the gap where it does not).

The corpus is four straight-line Rust files, one "arm" each:

| file | shape | what it proves |
|---|---|---|
| `tainted_handler.rs` | source, one function boundary, sink | the happy path: taint flows |
| `sanitized_handler.rs` | same shape, `escape_sql` on the path | a sanitizer stops the flow |
| `unrelated_handler.rs` | source and sink with no path | no path means no taint |
| `two_site_handler.rs` | one helper called from two sites, one tainted | the false-path control |

Run the pinned golden with `cd v6 && just cpg-taint-walk`. The graded rels and
their pinned byte offsets are in `2_expected.walk.tsv`.

## TOC

1. Who calls what, with which arguments
2. Find the source and the sink sites
3. The taint question: can the payload reach the sink
4. The two-site trap: one helper, two callers
5. Control-flow path and dominance

## How to read the answers

Every concrete span below is the `start..end` byte range that `2_expected.walk.tsv`
and `0_cpg_taint_walk.dl6` agree on. `var_read` is a dataflow node kind for an
identifier read; `call_res` is the dataflow node for a call's result. The
`L<line>:<col>` form is the same byte range mapped to a line and column.

The `cpg_edge_vocab.pl` rows are cited by line number. The report
`plans/2026-08-16-cpg-spec-research.REPORT.md` covers the CPG schema (edge and
node kinds), not the Scala query DSL, so query spellings below are best-effort;
anything I could not pin to the report is marked `UNVERIFIED`.

---

## 1. Who calls what, with which arguments

**Scenario.** A teammate wants a plain list of every function call in the corpus
and, for each, the expressions passed as arguments. This is the simplest thing
you ask a code graph, and the harder taint problems reuse this vocabulary.

**Joern concept.** The `cpg.call` traversal selects every CALL node. `.name`
reads the callee name property. `.argument` walks the ARGUMENT edge from a call
site to its argument expressions. This is where you first touch the two edges
the whole corpus hangs off: CALL and ARGUMENT.

**Edge rows exercised.** `cpg_edge(call, ...)` at `cpg_edge_vocab.pl:26`;
`cpg_edge(argument, ...)` at `cpg_edge_vocab.pl:27`.

**Joern query.**

```scala
cpg.call
  .map { c => (c.name, c.argument.code.toList) }
  .foreach(println)
```

**Expected answer on this corpus.**

| file | callee | arguments |
|---|---|---|
| `tainted_handler.rs` | `read_request_body` | none |
| `tainted_handler.rs` | `execute_sql` | `query_text` |
| `tainted_handler.rs` | `run_database_query` | `untrusted_payload` |
| `sanitized_handler.rs` | `escape_sql` | `untrusted_payload` |
| `sanitized_handler.rs` | `execute_sql` | `query_text` |
| `sanitized_handler.rs` | `run_database_query` | `sanitized_value` |
| `two_site_handler.rs` | `identity_passthrough` | `untrusted_payload` |
| `two_site_handler.rs` | `identity_passthrough` | `constant_statement` |
| `two_site_handler.rs` | `execute_sql` | `clean_echo` |
| `unrelated_handler.rs` | `execute_sql` | `constant_statement` |

`run_database_query` and `identity_passthrough` are the helper calls, so they
appear both as callee rows here and as the enclosing call when a handler invokes
them.

**The sprefa equivalent.** `call_site` (`0_cpg_taint_walk.dl6:116-120`) maps a
site span to its callee name; `df_arg` (`0_cpg_taint_walk.dl6:102-108`) maps a
call span to each argument by position. Both come straight off the
`--family call,df` wire and are emitted today. Answerable today.

---

## 2. Find the source and the sink sites

**Scenario.** Before asking whether data travels, you must know exactly which
lines hand untrusted data in and which lines consume it. The corpus encodes
this as two function names: `read_request_body` (the source) and `execute_sql`
(the sink).

**Joern concept.** Filtering a traversal by `name`, then walking `.argument` to
the argument expression. This is where you meet the two dataflow node kinds
behind every answer: the `call_res` node (a call's result) for the source, and
the `var_read` node (an identifier read) for the sink argument.

**Edge rows exercised.** `cpg_edge(call, ...)` at `cpg_edge_vocab.pl:26`;
`cpg_edge(argument, ...)` at `cpg_edge_vocab.pl:27`; the data-flow semantics of
`cpg_edge(reaching_def, ...)` at `cpg_edge_vocab.pl:25` that let a call result
be tracked as a value.

**Joern query.**

```scala
val sources = cpg.call.name("read_request_body").argument
val sinks   = cpg.call.name("execute_sql").argument
(sources.l, sinks.l)
```

**Expected answer on this corpus.** Four source nodes and four sink nodes, one
per file.

| rel | file | span | location |
|---|---|---|---|
| source | `tainted_handler.rs` | 373..392 | `handle_request` L16:29..46, `read_request_body()` result |
| source | `sanitized_handler.rs` | 441..460 | `handle_safe_request` L20:29..46 |
| source | `two_site_handler.rs` | 534..553 | `handle_two_site_request` L18:29..46 |
| source | `unrelated_handler.rs` | 303..322 | `handle_unrelated_request` L12:29..46 |
| sink | `tainted_handler.rs` | 298..308 | `run_database_query` L12:17..27, `query_text` arg |
| sink | `sanitized_handler.rs` | 361..371 | `run_database_query` L16:17..27, `query_text` arg |
| sink | `two_site_handler.rs` | 755..765 | `handle_two_site_request` L22:17..27, `clean_echo` arg |
| sink | `unrelated_handler.rs` | 395..413 | `handle_unrelated_request` L14:17..35, `constant_statement` arg |

**The sprefa equivalent.** `source_node` (`0_cpg_taint_walk.dl6:158-161`) and
`sink_node` (`0_cpg_taint_walk.dl6:163-167`) are literally this: both are
call-site name reads on `read_request_body` and `execute_sql`, emitted and
graded today. Answerable today.

---

## 3. The taint question: can the payload reach the sink

**Scenario.** The question the whole golden exists to answer: does any value
that `read_request_body` returns ever flow into an `execute_sql` argument? The
answer differs per file, and it is the first problem that is a real query rather
than a listing.

**Joern concept.** `reachableBy`, Joern's dataflow reachability query, and the
data-flow semantics of the REACHING_DEF edge: a definition reaches a use if it
is not reassigned on the way. This is the closest Joern idiom to sprefa's
`reaches` closure, and the sanitizer case below is where the two must agree.

**Edge rows exercised.** `cpg_edge(reaching_def, ...)` at `cpg_edge_vocab.pl:25`;
`cpg_edge(argument, ...)` at `cpg_edge_vocab.pl:27`.

**Joern query.**

```scala
cpg.call.name("execute_sql")
  .argument
  .reachableBy(cpg.call.name("read_request_body").argument)
  .l
```

`UNVERIFIED`: the exact `reachableBy` argument/result shape (a call, an argument
list, or a node) varies across Joern releases and is outside the vendored report,
which documents the schema, not the query DSL.

**Expected answer on this corpus.**

| file | does a source reach the sink | span it lands on |
|---|---|---|
| `tainted_handler.rs` | yes | 298..308, `query_text` var_read |
| `sanitized_handler.rs` | no | none |
| `unrelated_handler.rs` | no | none |
| `two_site_handler.rs` | naive yes, site-indexed no | 755..765, `clean_echo` var_read |

The negative arms are graded, not assumed: `sanitized_handler.rs` has
`escape_sql` on the path and `unrelated_handler.rs` has no path at all, and both
must come back empty.

**The sprefa equivalent.** `reaches`/`reach_hop` (`0_cpg_taint_walk.dl6:179-191`)
build the dataflow closure over `value_edge`, with the `not(sanitizer_node(Mid))`
stop at `0_cpg_taint_walk.dl6:190` cutting the sanitized arm; `tainted`
(`0_cpg_taint_walk.dl6:193-197`) is the source-to-sink projection. This is
answerable today by the existing taint walk: the `tainted` rows in
`2_expected.walk.tsv:34-35` pin `tainted_handler.rs` yes and both negative arms
empty.

---

## 4. The two-site trap: one helper, two callers

**Scenario.** `identity_passthrough` is called twice in `two_site_handler.rs`,
once with untrusted data and once with a constant. A context-insensitive return
edge lets the tainted value leave the helper through the clean call, so the clean
sink looks tainted. Only a call-return-indexed walk sees the truth.

**Joern concept.** Interprocedural dataflow precision: the return out of a
callee must be matched to the call site it entered through, the CFL call-return
discipline. The CALL and ARGUMENT edges give the call graph, but the question is
whether the return-to-caller hop stays site-matched.

**Edge rows exercised.** `cpg_edge(call, ...)` at `cpg_edge_vocab.pl:26`;
`cpg_edge(argument, ...)` at `cpg_edge_vocab.pl:27`; `cpg_edge(reaching_def, ...)`
at `cpg_edge_vocab.pl:25`.

**Joern query.**

```scala
// the naive reading: the tainted result of the first identity_passthrough
// call can return through the second, so clean_echo looks tainted
cpg.call.name("execute_sql").argument
  .reachableBy(cpg.call.name("read_request_body")).l
```

`UNVERIFIED`: whether Joern's default interprocedural dataflow is call-site
indexed the way this corpus demands. The point of the problem is the precision
distinction, not the exact default.

**Expected answer on this corpus.**

| walk | `two_site_handler.rs` sink | result |
|---|---|---|
| naive (`tainted`) | 755..765, `clean_echo` var_read | tainted, a false path |
| site-indexed (`site_tainted`) | 755..765 | refused |
| difference (`cfl_blocked`) | 755..765 | one row |

The single `cfl_blocked` row is `2_expected.walk.tsv:1`. It is the receipt that
the two walks disagree, and the gate (`3_gate.sh:195-207`) fails if they agree.

**The sprefa equivalent.** The whole point of the golden's second walk:
`top_tainted`/`top_step`/`call_tainted`/`call_step` indexed on the call site
(`0_cpg_taint_walk.dl6:204-233`), `site_tainted` (`0_cpg_taint_walk.dl6:235-241`),
and `cfl_blocked` (`0_cpg_taint_walk.dl6:245-248`) as the difference. Answerable
today by the existing taint walk.

---

## 5. Control-flow path and dominance

**Scenario.** A reviewer asks whether any control-flow path runs from the source
call to the sink call at all, and which statement must execute first, independent
of whether data flows. The corpus's extraction never emits control flow, so this
question cannot be asked of the data sprefa holds today.

**Joern concept.** `cfgNext` walks control-flow successors; `dominates` asks
whether one node must execute before another on every path. Both ride the CFG
and DOMINATE edges that Joern auto-derives from the METHOD entry and
METHOD_RETURN exit nodes (report: Cfg.scala:39-41, Ast.scala:393). The teaching
point: CFG reachability is necessary for taint but not sufficient, which is why
problem 4 needs dataflow and a site index on top of it.

**Edge rows exercised.** `cpg_edge(cfg, ...)` at `cpg_edge_vocab.pl:21`;
`cpg_edge(dominate, ...)` at `cpg_edge_vocab.pl:22`.

**Joern query.**

```scala
cpg.method.name("handle_request")
  .cfgNode
  .dominates
  .l
```

`UNVERIFIED`: the `cfgNode` / `dominates` step names.

**Expected answer on this corpus.** Each handler body is straight-line, so in
`tainted_handler.rs` `handle_request` (L15:4..18) the CFG is the chain
`read_request_body()` call (373..392) then `run_database_query(...)` call, and
the source call dominates the rest of the body. The limit shows up in
`two_site_handler.rs`: CFG reachability alone cannot separate the two
`identity_passthrough` calls, both of which are on a path to the sink, so
control flow alone cannot mark the false path that problem 4 removes.

**The sprefa equivalent.** Gap. sprefa emits no control-flow edges: `df_edge`
and the derived `value_edge` (`0_cpg_taint_walk.dl6:146-154`) are
REACHING_DEF-style dataflow, not CFG. The missing edge kind from the vocab enum
is `cpg_edge(cfg, ...)`, `cpg_edge_vocab.pl:21`, with `planned(cfg_edge)`
interface, plus `cpg_edge(dominate, ...)`, `cpg_edge_vocab.pl:22`, with
`planned(cdg_edge)` interface for the dominance half.

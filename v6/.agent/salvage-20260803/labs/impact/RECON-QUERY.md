# recon: what a query line actually does in v6, end to end

Scope: `/Users/chrishafley/projects/sprefa-recon-query`. Read-only; every claim carries
`file:line`. Budget of 3 quoted lines per receipt respected.

Terminology note: the surface the parser actually reads is `? Name(args).` with a SINGLE
question mark, not the brief's `?-`. Both names name the same statement; reports below use
"query line" and the AST term `query(Atom)`.

---

## parse

The query line is parsed in `v6/prolog/compile/parse_dl.pl`.

`query_stmt/5` consumes the literal `?`, then an identifier, an argument list, and a
closing `.`, building the atom `Atom =.. [Name | Positional]`:

- `v6/prolog/compile/parse_dl.pl:977-989`
  ```
  query_stmt(query(Atom), Vars0, Vars, S0, S) :-
      lit_dcg(`?`, S0, S1),
      ...
      Atom =.. [Name | Positional].
  ```

The whole file is collected as `program(Decls, Rules, Queries)`:

- `v6/prolog/compile/parse_dl.pl:122,136` `statements(...Queries)` and `Prog =
  program(Decls, Rules, Queries)`.

A file can hold MANY query lines: `statements/7` folds each query item onto the `Queries`
list.

- `v6/prolog/compile/parse_dl.pl:348` `Kind == query -> Decls = Decls1, Rules = Rules1,
  Queries = [Item | Queries1]`.

Real many-query file: `v6/tsv2/goldens/multirepo_crawl/0_multirepo_crawl.dl6:113-116` has
four `? dep_pin(...)`, `? skewed(...)`, `? skew_row(...)`, `? skew_width(...)` lines.

Registry classifies `query/1` as a readonly "decl" that lowers to a `query_plan`:

- `v6/prolog/compile/registry.pl:192` `surface(query/1, read, no_refs,
  decl(query_plan), live)`.

## expansion

`1_host_expand.pl` extracts queries from the parsed program and converts each to a plan;
this is the only expansion pass that touches query terms.

- `v6/prolog/1_host_expand.pl:40,54` `program_parts(Input, RawDecls, RawRules, Queries)`
  then `maplist(compile_query, Queries, QueryPlans)`.
- `v6/prolog/1_host_expand.pl:404-410`
  ```
  compile_query(query(Atom),
                query_plan(Name/Arity, columns(Args), snapshot(current))) :-
  ```
- `v6/prolog/1_host_expand.pl:59` the raw `query(Atom)` terms are APPENDED into the decl
  stream: `append([RawDecls, Queries, GeneratedDecls, UnprobedDecls, BindColumnDecls],
  Decls0)`.

In the compiler path the QueryPlans produced here are discarded outright:

- `v6/prolog/compile.pl:105-107` `prepare_program(SugaredProg, HostProg, _, _, _)` (the
  QueryPlans arg is `_`).

Queries are NOT typechecked like rule bodies. `0_program_check.pl`, `0_type_plane.pl`,
`lower.pl`, `analyze.pl`, and `1_expansion.pl` have zero matches for a `query(` term
(grepped, no hits). The `check_supported_subset_expanded` / `check_clock_program` gates in
`program_plan/2` run over `prog(Decls, Rules)` bodies only:

- `v6/prolog/compile.pl:128-135` `expand_program(HostProg, ExpandedProg, _)`,
  `check_supported_subset_expanded(Prog)`, `check_clock_program(Prog)`.

The query's referenced relation is not validated for existence, arity, or column types at
compile time by any query-specific check; the query atom travels inert.

## lowering + emit

`emit_ts.pl` re-derives the query plans from the decl stream (the raw `query(Atom)` terms
that `1_host_expand` embedded in `Decls`), keeping ONLY the relation name and arity:

- `v6/prolog/emit_ts.pl:310-314`
  ```
  findall(query_plan(Name/Arity, snapshot(current)),
          ( member(query(Atom), Decls),
            functor(Atom, Name, Arity) ),
          QueryPlans),
  ```
- `v6/prolog/emit_ts.pl:419-422`
  ```
  query_plan_json(query_plan(Name/Arity, snapshot(current)), Json) :-
      ...
      format(atom(Json), '{ rel: ~w, arity: ~w, snapshot: "current" }', ...)
  ```

The query atom's ARGUMENTS are dropped: nothing about `? change_log(Ep, _Kind, _Value)`
survives beyond `rel: "change_log", arity: 3`. `columns(Args)` from `compile_query` never
reaches emission.

Real generated output for a query-bearing fixture:

- `v6/tsv2/gen_emitted/native_ts_query_term.ts:56`
  `export const queryPlans: readonly IQueryPlanData[] = [{ rel: "captured", arity: 1,
  snapshot: "current" }];`
- `v6/tsv2/gen_emitted/struct_host_output_schedule_answer_interned.ts:59`
  `[{ rel: "host_start", arity: 2, snapshot: "current" }]`

The emitted TS for a query is therefore neither (a) a one-shot SELECT at boot, nor (b) a
standing subscription executed per tick. It is inert metadata. The engine's `tick` runs the
full incremental cascade over every level rule regardless of which rels any query names:

- `v6/tsv2/gen_emitted/native_ts_query_term.ts:390-396`
  ```
  function runTick(seam, arrivals) {
    arrivals = validateArrivals(arrivals);
    ... return runIncrementalTick(seam, arrivals);
  }
  ```
- `v6/tsv2/gen_emitted/native_ts_query_term.ts:376-387` `runIncrementalTick` applies
  `INCREMENTAL_LEVEL_STATEMENTS` (one entry per rule, every rule in the program) on every
  tick.

A per-rel read `SELECT` exists in `finalSelect` (e.g. `native_ts_query_term.ts:262` for
`captured`), used only by the pull-side `rows(rel)` API; it is not invoked by the tick.

## runtime

`v6/tsv2/serve/3_engine.ts` is the live loop. `ticks$` turns only while subscribed, and
each enqueued batch runs the full program tick:

- `v6/tsv2/serve/3_engine.ts:102-113` `this.ticks$ = this.arrivals.pipe(concatMap(...))`.
- `v6/tsv2/serve/3_engine.ts:191` `return this.program.tick(this.seam, arrivals).pipe(...)`.

Query results are computed because EVERY level rule runs every tick; there is no lazy
machinery that computes a rel only when a query demands it. `queryPlans` is never read by
the runtime: the only non-generated references are a field declaration and a test literal.

- `v6/tsv2/runtime/types.ts:438` `readonly queryPlans: readonly IQueryPlan[];`
- `v6/tsv2/tests/6_host-extraction-batching.test.ts:119` `queryPlans: [],`

A grep for `.queryPlans` across `runtime/` and `serve/` returns nothing; `LiveEngine` reads
`program.finalSelect`, `relColumns`, `relColumnTypes`, `ddl`, and `boot`, not
`program.queryPlans` (`3_engine.ts:127-133,220-233`).

The vocabulary "demand" in the codebase refers to external-host input demand
(`__host_demand_*` rows produced by rules that probe an `sh` host), not to query demand;
those rows are generated unconditionally by level rules (see
`native_ts_query_term.ts:322` `__host_demand_tree_sitter` level) with no dependency on
`queryPlans`. There is no demand graph, refCount, or query-keyed topo/stratum scheduling.
The stratum order is a fixed rule ordering from `sql_rule_order`; it does not change with
queries.

## oracle

The Prolog reference engine evaluates all rules every tick and reports the union of all
relations; queries are not demand roots and are never consulted.

- `v6/prolog/conformance/engine.pl:503-531` `run_program/5` calls
  `prepare_program(SugaredProg, HostProg, _, _, _)` (QueryPlans discarded at
  `engine.pl:504`), then `level_closure(PlainLevel, AggRules, ...)` over every rule
  (`engine.pl:529`) and `run_ticks` which recomputes every tick.
- `v6/prolog/conformance/engine.pl:558-560` the final result is the sorted union of all
  stores: `store_rows(Store, Rows), append(Rows, Level, FinalAll0), msort(FinalAll0,
  FinalAll)` -- nothing filters it to query-named rels.

No `query(` term appears in `engine.pl` or `level_eval.pl` (grepped, no hits). The oracle
parses/expects `final(Ref, Rows)`, `deltas(Ref, ...)`, `ticks(N)` in fixture expectations
(`engine.pl:578,604`), all keyed on whatever rels the test asserts, independent of query
lines. Oracle behavior = "evaluate all rules, report rels named in expectations", not
queries-as-demand.

## ddl

All table definitions are created unconditionally at boot, regardless of any query.

- `v6/tsv2/serve/3_engine.ts:220-232` `bootServedProgram` runs
  `[...program.ddl, ...WitnessCache.ddl()]` then `BootRunner.run(seam, program.boot)`.
- `v6/tsv2/gen_emitted/native_ts_query_term.ts:133-193` the `ddl` array `CREATE TABLE`s
  every rel plus every delta/frontier/support working table, including rels no query names
  (`file_digest`, `interval`, `query_source`, `query_value`, and all `__host_*`).

The query line contributes nothing to the DDL set; every rel the program declares or any
rule writes gets a table whether or not a query references it.

## serve

Serving is a push/subscription architecture (rxjs observables + a `Subject`), re-running
the full incremental tick when rows arrive; it does not poll for queries.

- `v6/tsv2/serve/4_http.ts:426` `POST /arrivals` → `engine.submit(checked.batch)` (the
  submit path at `4_http.ts:322`).
- `v6/tsv2/serve/2_binds.ts:4-5` live `interval` and `watch` sources each `map(...) ->
  mergeMap(submit)`: `interval(periodMs) -> map(toBucketRow) -> mergeMap(submit)`,
  `watchSource(root) -> bufferTime(coalesceMs) -> map(diffAgainstLast) -> submit`.
- `v6/tsv2/serve/1_hosts.ts:690` host responses re-enter through the same seam:
  `this.engine.submit(arrivals)`.

Every pushed batch triggers `program.tick` over all levels (`3_engine.ts:188-208`), which
recomputes and diffs every queried and unqueried rel and emits deltas on `ticks$`. No
per-query re-SELECT loop exists; `LiveEngine.rows(rel)` (`3_engine.ts:127-134`) is a
pull-only read of `finalSelect` and is not wired to any per-query subscription that
re-emits on new rows.

## verdict

The belief "?- is the subscribe, everything else is lazy" is **FALSE** as built, with one
true fragment. TRUE fragment: compilation is eager -- all table defs are created up front
at boot (`3_engine.ts:224`, `ddl` array `native_ts_query_term.ts:133-193`), and no query
becomes a one-shot boot SELECT. FALSE halves (the core of the belief): a query line is
not the subscribe operation, is not the only demand root, and no laziness/demand machinery
exists. A query line lowers to dead metadata `{ rel, arity, snapshot }`
(`emit_ts.pl:310-314,419-422`; `gen_emitted/native_ts_query_term.ts:56`) whose arguments
are dropped, and `queryPlans` is never read by the runtime (`grep` over
`runtime/ serve/`; only `types.ts:438` declares it). Nothing "clocks" on a query: the
engine recomputes EVERY rule every tick (`3_engine.ts:191` → `runIncrementalTick`,
`native_ts_query_term.ts:376-387`) and eval is driven by world arrivals pushed over
HTTP/bind/host (`4_http.ts:322,426`, `2_binds.ts:4-5`, `1_hosts.ts:690`), not by queries.
The oracle matches that: it evaluates all rules every tick and reports all rels, never
keying off queries (`engine.pl:503-531,558-560`).

Smallest change set to make "?- is the subscribe, everything else is lazy" true:

- Keep the query atom's argument columns through emission so a projection and a demand
  root can be formed. **Files:** `v6/prolog/1_host_expand.pl:404-410`
  (`compile_query`), `v6/prolog/emit_ts.pl:310-314,419-422` (`world_plan_lines`,
  `query_plan_json`), and the emitted `IQueryPlanData` shape at
  `v6/prolog/emit_ts.pl:288`.
- Compute a query-reachable demand closure and prune level statements, host-demand
  generation, and unnecessary tables to that closure, keyed off `queryPlans`. **Files:**
  `v6/prolog/lower.pl` (level/edge statement generation and `boot_statements`),
  `v6/prolog/analyze.pl:124-175` (`program_plan` rel/rule collection), and the generator
  that assembles `INCREMENTAL_LEVEL_STATEMENTS` / `ddl` in `v6/prolog/emit_ts.pl`.
- Consume `queryPlans` in the served runtime and expose per-query standing streams that
  re-emit the query rel's `finalSelect` rows on each tick's deltas. **Files:**
  `v6/tsv2/serve/3_engine.ts` (subscribe/emit query results), `v6/tsv2/serve/4_http.ts`
  (query-result endpoint), `v6/prolog/emit_ts.pl` (still carry the projected columns).

That set changes only the query path; the non-query push/recompute pipeline above is left
as-is. No design opinion intended beyond this list.

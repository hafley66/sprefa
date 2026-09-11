# 27. V7 SWI-Prolog gotcha audit

Date: 2026-09-11. Reviewer: native Astra. Scope: all 67 V7 `.pl` files, 24,918 lines at final inventory. Every file was read completely, including tests, benchmarks, examples, and labs. Source and tests were read-only. This receipt adds no CI coverage and makes no kernel changes.

Read root and V7 `AGENTS.md`, `swi-prolog-performance-jutsu/SKILL.md`, and its measured-patterns reference. The skill determined the serial, bounded probes, explicit modes, and separation of measured cost from source-derived complexity. Native delegation follows the user's temporary Boop override.

Evidence labels:

- **MEASURED**: executed a targeted probe against this checkout.
- **SOURCE VERIFIED**: the causal path follows directly from the implementation. Runtime manifestation need not have been exercised.
- **HYPOTHESIS**: requires additional workload evidence or a boundary decision.

## Measured inference contribution

One fresh SWI process compiled `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7` with `DL7_TRACE=off`, empty compiler caches, and targeted `library(prolog_wrap)` wrappers. Result: **810 rows, no diagnostics, 2,933,144 instrumented inferences**. Each wrapper counted inclusive inferences across outermost calls to that predicate; recursive calls were not double-counted. These include wrapper overhead and are not uninstrumented benchmark results. Percentages use that one instrumented compile as denominator. Wall time is omitted because this audit ranks inference work.

| Rank | Predicate | Calls | Inclusive inferences | Compile share |
| --- | --- | ---: | ---: | ---: |
| 1 | checker `dense_index_diagnostics/4` | 4 | 538,594 | 18.36% |
| 2 | evaluator `demand_cone_rules/6` | 15 | 431,151 | 14.70% |
| reference | evaluator `collect_closure/4` | 15 | 173,510 | 5.92% |
| 3 | evaluator `worklist_loop/5` | 4 | 22,005 | 0.75% |
| outside this fixture | syntax `materialize_syntax/4` | 0 | 0 | 0% |
| outside this fixture | extract `accepted_rows/2` | 0 | 0 | 0% |

`bind_diagnostics/3` cost 553,468 inclusive inferences across four calls. It contains the dense-index cost above; those numbers must not be added. Collection is shown as a reference measurement, not an additional open finding. Its ownership remains with the closure lanes, which received these results.

Synthetic scaling used one process, lists created before each inference counter, and sizes 100, 200, 400. No source was replaced with a proposed implementation.

| Target | 100 | 200 | 400 | Input |
| --- | ---: | ---: | ---: | --- |
| dense indices | 11,003 | 42,002 | 164,002 | one edge per distinct integer owner, index 0 |
| worklist | 6,810 | 23,302 | 86,602 | N queued keys, zero levels, self-reader gap 0 |
| materializer | 287,759 | 1,134,231 | 4,508,431 | N atom frontiers, four rows per node |
| TSI acceptance | 72,873 | 285,416 | 1,130,816 | one syntax run, N facts and N witnesses |

The first case of each synthetic series can include autoload/JIT warmup. The growth claim also follows from explicit list traversals below.

## Findings

### 1. Dense index checking recounts every owner for every edge

**MEASURED + SOURCE VERIFIED.** [1_checker.pl](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:338), `dense_index_diagnostics(+Edges,+AllEdges,+Origins,-Diagnostics)` and `count_owner_edges(+AllEdges,+Owner,-Count)` at line 351.

`check_datalog_body/4` calls `bind_diagnostics/3`; each pending edge calls `count_owner_edges/3` over the entire original edge list. With E edges this visits exactly E² edge-list cells, even when every owner has one edge. Origin lookup is additional work. Four invocations account for the largest measured candidate contribution above.

Smallest preserving correction: construct an owner-count assoc once in `bind_diagnostics/3`, then look up the count for each edge. Preserve edge iteration order, duplicate diagnostics, and origin selection. A singleton-owner fixture proves the count needs no repeated traversal. No change to accepted DL7 forms is required.

### 2. Demand-cone expansion rejoins already selected heads on each pass

**MEASURED + SOURCE VERIFIED.** [0_evaluator.pl](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:190), `demand_cone_rules(+Strata,+Level,+Rules,+Dependencies,+CurrentRules,-PlainRules)`; fixpoint body at line 197.

Each pass enumerates every selected rule, then scans all dependencies for its head, sorts the resulting body relations, scans all rules, and sorts the enlarged selected set. If a head has K selected rule definitions, its dependency scan repeats K times. For passes p, the join contributes `sum_p(selected_rules_p * dependency_count)` list visits, before rule selection and sorting. A chain added one definition per pass can make this term cubic in chain length.

Smallest preserving correction: index plain-positive dependencies by exact head and eligible plain definitions by exact relation for this evaluation; visit each newly discovered relation head once and retain final `sort/2` of selected rules. Preserve aggregate exclusion, strict/negative snapshot reads, and `RuleLevel =< Level`. Evaluator implementation requires the user's kernel-participation approval before a patch.

### 3. Worklist copies the remaining queue when no item was enqueued

**MEASURED + SOURCE VERIFIED.** [0_evaluator.pl](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:784), `worklist_loop(+Queue,+DependencyIndex,+Levels0,-Levels,+Debug)`.

When a queued relation has a dependency-index entry, line 792 calls `append(Queue0, Enqueued, Queue)`. For zero-gap satisfied readers, `Enqueued=[]`, but `append/3` still copies `Queue0`. N such queue entries cause N(N-1)/2 copied cells. The synthetic self-reader case isolates this without any level changes.

Smallest preserving correction: reuse `Queue0` when `Enqueued == []`; otherwise retain existing append and FIFO ordering. A difference-list queue is a separate, larger patch. Measured nearest-shadow contribution is 0.75%, below findings 1 and 2.

### 4. Materialization repeatedly scans the entire syntax-row bag

**MEASURED + SOURCE VERIFIED.** [1b_syntax_materializer.pl](/Users/chrishafley/projects/sprefa/v7/src/0_reader/1b_syntax_materializer.pl:43), `materialize_syntax(+ground Rows,-Forms,-SourceRows,-Diagnostics)` and `materialize_node(+Node,+Rows,+Seen0,-Seen,-Result,-Sources,-Diagnostics)`.

Per reachable node, node presence scans Rows once, four payload alternatives each scan Rows, and source lookup scans Rows once. Each form performs another complete edge scan at line 118. The Seen list adds repeated membership scans. With V nodes and R rows, the row lookup portion is at least six V×R list traversals, plus form-edge scans; R=4V in the measured atom-frontier case.

Smallest preserving correction: create per-node immutable indexes for node occurrences, payloads, sources, and child edges. Preserve raw duplicate node/source occurrences because `expected_one` and `source_rows` diagnostics depend on them; preserve current payload deduplication separately. Do not indiscriminately sort the whole input into a set. This path was not reached by the measured nearest-shadow compile.

### 5. TSI acceptance recomputes witness/run/completeness joins per fact

**MEASURED + SOURCE VERIFIED.** [0c_extract_loader.pl](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/0c_extract_loader.pl:288), `accepted_rows(+ground Rows,-Accepted)`, `accepted_fact(+Rows,+Fact,+Relation)` at 298, `complete_semantic_runs(+Rows,+Scope,+Relation,-Runs)` at 350.

Each fact scans witnesses, runs, and complete semantic runs. Complete-run discovery itself joins all run rows against coverage rows and sorts. Even one syntax run gives quadratic scaling over N facts and witnesses; with many semantic runs, repeated run×coverage joins add another multiplier. Both expression-environment construction and graph installation call `accepted_rows/2`.

Smallest preserving correction: index witnesses by fact and metadata by run; compute complete runs once per exact Scope-Relation within `accepted_rows/2`. Keep highest-run selection and the current rule accepting semantic runs when no complete claim exists. Keep final exact-row sort. This path was not reached by the measured nearest-shadow compile.

### 6. Trace-output exceptions leave an active trace marker behind

**MEASURED + SOURCE VERIFIED.** [1b_compiler_tracer.pl](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1b_compiler_tracer.pl:86), `finish_compile_trace(+Program,+Before)`; public `with_compile_trace(+Program,:Goal)` at 66.

The finalizer writes debug/summary/JSON before `reset_compile_trace/0`. A failed output operation throws before the reset. The next compile sees `active_compile_trace/1` and takes the nested-frame branch, bypassing outer trace initialization/finalization.

Probe: set `DL7_TRACE=json` and `DL7_TRACE_FILE=/dev/null/astra-audit.jsonl`, then catch `with_compile_trace(audit,true)`. Observed `existence_error(directory,'/dev/null')` and **one remaining active trace fact**. The probe explicitly reset it afterward. Existing trace tests cover exceptions in Goal, not exceptions in the output cleanup.

Smallest preserving correction: put finalizer reporting inside a cleanup construct whose unconditional cleanup resets trace state; preserve the reporting exception. This is one failed-output event followed by persistent misclassification of subsequent compile calls in the same thread.

### 7. Checker debug fallback adds a second complete checker solution

**MEASURED + SOURCE VERIFIED.** [1_checker.pl](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:99), `debug_checker_input(+Basement,+Origins)` and public `check_datalog(+Basement,+Origins,-Checked,-Diagnostics)`.

The structured-input debug clause succeeds without a cut, and the fallback at line 116 also succeeds. Backtracking through `check_datalog/4` reruns the check body. Probe:

```prolog
findall(D, dl7_checker:check_datalog(
    basement_program(root_graph([],[]),datalog_program([],[],[])),
    [], _, D), Ds).
% Ds = [[], []].
```

Multiplicity: exactly two checker paths from this debug helper for the empty valid input, despite the public `is det` comment. This can also delay `setup_call_cleanup/3` cleanup until the retained choicepoint is discarded. Normal compiler callers often commit to the first solution, which hides it.

Smallest preserving correction: commit after matching the structured debug-input head, leaving the fallback for other shapes. Add a determinism/solution-count regression covering trace on and off, not just equal first-result data.

### 8. Empty-tail cons construction has two proofs of one tuple

**MEASURED + SOURCE VERIFIED.** [0_evaluator.pl](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:1202), `cons_construct(+ground Head,+ground Tail,-List)` via `cons_relation/3` construction mode.

Both the special empty-tail clause and the general `is_list(Tail)` clause accept `const([])`. Probe `findall(L, dl7_evaluator:cons_relation(const(a),const([]),L), Ls)` returns `[const([const(a)]),const([const(a)])]`.

Multiplicity: two raw solutions per singleton construction. Tabled callers deduplicate the tuple, so this probe does not establish duplicate final closure rows. The comment says semidet, while this construction mode retains a second proof.

Smallest correction: remove the special clause already covered by the general proper-list clause. This preserves the tuple set; it changes raw proof multiplicity. Obtain kernel participation approval before implementation.

### 9. Exported diagnostic runner loses caller module context

**MEASURED + SOURCE VERIFIED.** [0_compiler_performance.pl](/Users/chrishafley/projects/sprefa/v7/bench/0_compiler_performance.pl:392), `diagnostic_outcome(+LimitSeconds,:Goal,-Status)` and `diagnostic_outcome(:Goal,-Status)` at 401.

The exported predicates call a goal but have no `meta_predicate` declarations. From a separate module that imports the runner and defines a private `local_probe/0`, `diagnostic_outcome(local_probe, Status)` returns `exception(error(existence_error(procedure,dl7_compiler_performance:local_probe/0),...))`. Existing tests use built-ins (`true`, `fail`, `throw`, `repeat`), which do not expose the module error.

Multiplicity: one callback resolved in the wrong module on each affected call. Smallest preserving correction: declare `diagnostic_outcome(+,0,-)` and `diagnostic_outcome(0,-)` as meta-predicates. Add an imported-runner/private-callback test. The currently internal benchmark diagnostic compile callback already resides in the benchmark module.

### 10. Subprocess wrappers can block on undrained pipes and skip cleanup

**SOURCE VERIFIED; no deadlocking child launched.**

- [3_rust_type_region_mainer.pl](/Users/chrishafley/projects/sprefa/v7/src/3_emit/3_rust_type_region_mainer.pl:67), `run_process(+Executable,+Arguments,+Input,-Exit,-Output,-Error)`.
- [2_dl7_rust_region_mainer.pl](/Users/chrishafley/projects/sprefa/v7/src/4_tool/2_dl7_rust_region_mainer.pl:43), `run_region/6`, same modes.
- [0_source_query_mainer.pl](/Users/chrishafley/projects/sprefa/v7/src/4_tool/0_source_query_mainer.pl:94), `run_process(+Executable,+Arguments,-Exit,-Output,-Error)`.

The parent reads stdout until EOF before draining stderr. A child writing more than its stderr pipe capacity can block before closing stdout; the parent waits for stdout EOF. The region wrappers first synchronously write all stdin, introducing a second cycle if the child writes enough output before consuming its input. Exact byte threshold is OS/runtime dependent and was not measured.

These wrappers also lack unconditional stream/process cleanup if writing or reading throws. The region variants match only `process_wait(Pid,exit(Exit))`; a signaled child yields `killed(Signal)`, making the wrapper fail instead of reaching its result diagnostic. The source-query wrapper already handles this status.

Smallest preserving correction: reuse the repository's existing file-spooled subprocess pattern from `4_tool/5_extract_sqlite_query_mainer.pl`, with owned temporary input/output/error resources and exception cleanup/reaping; normalize signaled exit status before decoding. Preserve the subprocess argument list and output bytes. Similar sequential-pipe helpers appear in entrypoint, watch, SQLite, and CLI tests, so tests can inherit the same hang shape.

### 11. Zero-column SQLite output is accepted but renders invalid SQL

**MEASURED + SOURCE VERIFIED.** [1c_sqlite_query_emitter.pl](/Users/chrishafley/projects/sprefa/v7/src/3_emit/1c_sqlite_query_emitter.pl:117), `normalize_output_layout(+Layout,-Output)` through `emit_sqlite_query(+Compiled,+Layout,-Artifact,-Diagnostics)`.

A checked program with unary `Input`, zero-arity `Output`, and rule `Output() <- Input(x)` accepts layout `sqlite_output("Output",[])`. Probe result:

```text
Diagnostics = []
SELECT DISTINCT  FROM "input" AS "g0" WHERE "g0"."x" IS NOT NULL
```

SQLite rejected this exact SQL with `near "FROM": syntax error`. Multiplicity is one invalid select per affected rule. Empty-body-rule rejection does not cover zero-column heads.

Smallest correction preserving all currently valid SQL output: emit an explicit unsupported-zero-column diagnostic. Supporting zero-arity tuples with a sentinel column requires an output-contract decision. Do not alter DL7's zero-arity relation semantics to repair the backend boundary.

### 12. Rust identifier checks accept keywords that are emitted as syntax

**MEASURED emitted text + SOURCE VERIFIED; Rust compile not run.** [2a_dl7_rust_emitter.pl](/Users/chrishafley/projects/sprefa/v7/src/3_emit/2a_dl7_rust_emitter.pl:157), `rust_identifier(+Name,-Identifier)`, `rust_field_name(+Name,-Escaped)`, public `render_dl7_rust_program(+Path,+Checked,-Text,-Diagnostics)`.

Identifier validation checks only character classes. Type/variant names are emitted unchanged; field escaping uses the twelve-fact keyword list at 181. A checked product named `fn` with fields `let` and `self` yielded no diagnostics and:

```rust
pub struct fn {
    pub let: i64,
    pub r#self: i64,
}
```

Multiplicity: every occurrence of an affected name produces the same invalid token; type references repeat it. `_` is also admitted as a name by the character predicate.

Smallest correction preserving existing valid output: validate/encode identifiers according to their target position, including raw-identifier exceptions, and diagnose unsupported names. Use one complete target-language keyword policy across type, field, and variant positions. Existing test 10 covers only the schema golden file.

### 13. A profile hierarchy test quantifies over category but not stratum

**SOURCE VERIFIED.** [21_compiler_profile.test.pl](/Users/chrishafley/projects/sprefa/v7/test/21_compiler_profile.test.pl:216), body of `test(json_has_expected_hierarchy)`; intended inputs are ground profile Spans and StratumIds.

`forall(member(ChildCategory,[install,collect,cleanup]), (member(StratumId,StratumIds), ...))` proves that each category occurs under some stratum. It does not prove the adjacent comment's claim that every stratum has all three children. For S strata the intended coverage is 3S checks; the generator performs three checks with existential stratum selection. One complete stratum can mask missing children on every other stratum.

Smallest preserving correction: put both stratum and category enumeration in the `forall/2` generator. Add a synthetic hierarchy missing one child's span to verify this test's failure path. This affects CI coverage rather than compiler behavior.

## Hypotheses and explicitly unclaimed issues

- **HYPOTHESIS:** logical-row reification and SQL/DBSP reconstruction repeatedly scan whole logical programs for call arguments, protocol relations, and field names. Those paths were read, but this audit did not profile an emitter-heavy fixture. Do not rank their CPU contribution using row-duplicate percentages.
- **HYPOTHESIS:** process-local compiler caches retain distinct source revisions until `clear_compiler_caches/0`. Long-running memory impact needs a bounded revision workload and a user-selected retention policy. Their documented lifetime alone does not establish a defect.
- Evaluator clause installation performs assertions during `setup_call_cleanup/3` setup. A setup exception after a partial install deserves a dedicated fault-injection audit. The initial candidate of malformed ground public seed input was withdrawn: outer seed selection filters that shape before installation. No public normal-input leak was demonstrated.
- No recommendation to change table modes or add subsumption: `proves/2` is variant-tabled; current collection binds result relations; evaluation-ID-specific tables are abolished by cleanup. Existing indexed lookup rechecks exact terms after hashing and hashes only ground arguments. No hash-collision correctness defect was found in that path.
- Current evaluator table metrics are named `global_*`; they describe process/thread table state, not isolated stratum allocations. The audit did not reinterpret them as per-stratum memory.
- Lowerer/checker owner and name traversal includes visited-state handling; existing lexical tests exercise self-cycle, two-cycle, nearest shadow, and generated callable identities. No additional verified owner-traversal defect resulted from this read.
- Standalone lab initialization/halting is confined to lab scripts; tool mains predominantly use `initialization(main,main)`. No additional module-import startup defect was verified.

## Next two bounded patches

1. **Checker owner-count index**, finding 1. Only count construction and lookup change. Add exact diagnostic snapshots for dense, duplicate, negative, and gapped indices; assert checker output parity on nearest-shadow. Measure the same cold predicate and compile counters. The measured candidate share is 18.36% inclusive, not a promised whole-compile speedup.
2. **Demand-cone relation indexes**, finding 2, after the required evaluator explanation and approval. Preserve exact sorted selected rule sets and all existing positive/negative/aggregate closure fixtures. Measure cone calls separately from collection. The measured candidate share is 14.70% inclusive.

Trace finalizer cleanup (6), the checker choicepoint (7), callback qualification (9), and subprocess cleanup (10) are separately bounded correctness/operational patches. Their priority is not inferred from inference share. None was implemented here.

## Read inventory, dependency/reading order

Directories follow repository layer order; tracer/cacher are shared infrastructure dependencies of later layers despite their numeric placement. Imports contain cross-layer links, so this is a reading order rather than a claimed strict topological order.

```text
v7/src/0_reader/
  0_parser.pl
  1_expander.pl
  1a_syntax_grapher.pl
  1b_syntax_materializer.pl
  2_embedder.pl
  3_file_loader.pl
  4_module_loader.pl
  5_cli_mainer.pl
v7/src/1_libtime/
  0_evaluator.pl
  0a_syntax_macro_program.pl
  0b_syntax_rewriter.pl
  1_syntax_expander.pl
v7/src/2_comptime/
  0_lowerer.pl
  0a_module_lowerer.pl
  0b_filesystem_grapher.pl
  0c_extract_loader.pl
  0d_source_fact_loader.pl
  1_checker.pl
  1a_generated_program_assembler.pl
  1b_compiler_tracer.pl
  1c_compiler_cacher.pl
  1d_host_planner.pl
  2_compiler.pl
v7/src/3_emit/
  0_logical_program_reifier.pl
  0a_logical_program_grapher.pl
  1_artifact_emitter.pl
  1a_dbsp_plan_emitter.pl
  1b_dbsp_rust_emitter.pl
  1c_sqlite_query_emitter.pl
  2_rust_type_emitter.pl
  2a_dl7_rust_emitter.pl
  3_rust_type_region_mainer.pl
v7/src/4_tool/
  0_source_query_mainer.pl
  1_dbsp_plan_mainer.pl
  2_dl7_rust_region_mainer.pl
  3_dbsp_rust_region_mainer.pl
  4_sqlite_query_mainer.pl
  5_extract_sqlite_query_mainer.pl
v7/bench/
  0_compiler_performance.pl
  1_compiler_profile.pl
v7/test/
  fixtures/1_embedded.pl
  0_reader.test.pl
  1_entrypoints.test.pl
  1a_syntax_expander.test.pl
  2_module_system.test.pl
  3_compiler_trace.test.pl
  4_extract_loader.test.pl
  5_source_fact_loader.test.pl
  6_rust_type_emitter.test.pl
  7_rust_type_region.e2e.pl
  8_source_query.test.pl
  9_dbsp_plan.test.pl
  10_dl7_rust_emitter.test.pl
  11_dbsp_rust_emitter.test.pl
  12_watch_pipeline.e2e.pl
  13_sqlite_plan.e2e.pl
  14_sqlite_query_emitter.test.pl
  15_interned_storage.test.pl
  16_storage_plain_field.test.pl
  17_dl6_interned.test.pl
  18_binding_symmetry.test.pl
  19_lexical_binding.test.pl
  20_compiler_performance.test.pl
  21_compiler_profile.test.pl
v7/applications/dl6/1_demo.pl
v7/labs/14_binary_packaging/5_MINIMAL_SWI.pl
v7/labs/18_runtime_shootout/2_swi.pl
```

## Probe record

Temporary read-only probe harness: `/private/tmp/dl7_astra_audit_probe.pl`. Each run used `timeout 15s swipl -q -s /private/tmp/dl7_astra_audit_probe.pl -g main -t halt -- MODE`; modes were `correctness`, `profile`, `scaling`, and `emitters`. No SWI processes overlapped. Parent explicitly granted and then received release of the serial SWI window. The first callback probe accidentally exported its callback, making it inherited through `user`; it was corrected to a private callback and rerun. Only the corrected result appears in finding 9.

The SQL parser rejection was a separate in-memory `sqlite3 :memory:` query. No broad test suite, Rust compilation, destructive operation, commit, or push was performed. Existing dirty work was preserved; source changes by the closure lane were audited as the current checkout. This receipt reports targeted current observations, not a claim that all CI suites pass.

# 34. Current-head predicate attribution

Date: 2026-09-11. Audited HEAD: `70b250b3d3ec87b8f9b280507d2a304efc7a271b`.
Fixture: `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`.
Read-only native Astra profiling. Source and tests were not edited.

## Current result

A fresh, unwrapped process with `DL7_TRACE=off` and cleared compiler caches
compiled **810 rows with no diagnostics in 2,010,865 inferences**.
Observed compile wall time was 426.804 ms. This is a single observation, not a wall
budget or a throughput estimate.

Canonical `result(Rows,Runtime,Diagnostics)` SHA-256:

```text
f42d4b6a73ce4cdd97594ff9710377c499a8c78e2ff8987517793cbbca29a99a
```

Every single-target attribution pass and both causal probes below produced that
same hash. All SWI processes ran serially with an external 15-second timeout.
Compiler caches were cleared inside each fresh process.

The SWI performance skill determined the narrow measurements, supported-mode
checks, and explicit separation of inference work, C-builtin cost, and phase
intervals. Its historical audit measurements were not reused as current results.

## Top 10 inclusive costs

Rows 1 through 10 below are the ranked result. The extended table retains the
requested lower-ranked checker, evaluator, reader, grouping, and indexing
measurements. These are the top targets among 52 screened predicate candidates;
this is not an exhaustive census of every internal predicate.

Each row is from **its own fresh process with only that predicate wrapped**.
This avoids charging a parent for wrappers installed on all its small children.
A simultaneous broad pass was used only for screening and was discarded for the
final ranking.

`Calls / outer` means all wrapper entries, including recursive ones, followed by
the number of outermost timed calls. Recursive calls contribute to the outer
call's inclusive work once. For nondeterministic goals, `call_time/3` records
each answer/failure segment so unrelated caller continuation is not included.
These are predicate-entry counts, not distinct table or unique-input counts.

Compile share uses **that row's instrumented compile total**, shown explicitly.
A few-off `call_time/3` inference correction can be negative for trivial/foreign
calls; negative segments were clamped to zero. Small numbers and zeros therefore
have limited precision. Wrapper overhead remains, especially on recursive or
frequently called targets; the unwrapped total above is the authoritative compile
counter.

| Rank | Predicate | Calls / outer | Inclusive inferences | Own-pass compile inferences | Compile share |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | `dl7_evaluator:evaluate/4` | 3 / 3 | 489,188 | 2,010,971 | 24.33% |
| 2 | `dl7_parser:read_dl7/5` | 3 / 3 | 378,841 | 2,010,971 | 18.84% |
| 3 | `dl7_checker:check_resolved_rules/5` | 5 / 5 | 279,671 | 2,011,037 | 13.91% |
| 4 | `dl7_checker:check_datalog/4` | 4 / 4 | 276,172 | 2,011,004 | 13.73% |
| 5 | `dl7_lowerer:lower_datalog_mode/6` | 6 / 6 | 178,388 | 2,011,070 | 8.87% |
| 6 | `dl7_evaluator:collect_closure/4` | 15 / 15 | 173,421 | 2,011,367 | 8.62% |
| 7 | `dl7_evaluator:proves/2` | 3806 / 167 | 170,457 | 2,105,371 | 8.10% |
| 8 | `dl7_checker:resolve_rules/8` | 268 / 4 | 160,632 | 2,012,852 | 7.98% |
| 9 | `dl7_syntax_grapher:reify_syntax/4` | 4 / 4 | 142,676 | 2,011,004 | 7.09% |
| 10 | `dl7_checker:check_goal_sequence_failures/7` | 2224 / 648 | 138,573 | 2,043,288 | 6.78% |
| 11 | `dl7_checker:head_safety_diagnostics/5` | 648 / 648 | 120,042 | 2,032,256 | 5.91% |
| 12 | `dl7_lowerer:lower_executables/6` | 6 / 6 | 116,662 | 2,011,070 | 5.80% |
| 13 | `dl7_evaluator:stratify_rules/3` | 9 / 9 | 112,022 | 2,011,169 | 5.57% |
| 14 | `dl7_evaluator:install_evaluation/5` | 15 / 15 | 110,881 | 2,011,367 | 5.51% |
| 15 | `dl7_evaluator:install_lower_rows/3` | 15 / 15 | 100,366 | 2,011,367 | 4.99% |
| 16 | `dl7_evaluator:install_shared_lower_rows/3` | 15 / 15 | 100,336 | 2,011,367 | 4.99% |
| 17 | `dl7_expander:expand_dl7/6` | 3 / 3 | 97,269 | 2,010,971 | 4.84% |
| 18 | `dl7_evaluator:index_argument_hashes/5` | 3782 / 3782 | 81,173 | 2,135,678 | 3.80% |
| 19 | `dl7_evaluator:validate_functional_rows/3` | 4 / 4 | 64,165 | 2,011,004 | 3.19% |
| 20 | `dl7_evaluator:demand_cone_rules_indexed/4` | 15 / 15 | 32,697 | 2,011,367 | 1.63% |
| 21 | `dl7_evaluator:demand_cone_worklist/8` | 385 / 15 | 32,196 | 2,013,957 | 1.60% |
| 22 | `pairs:group_pairs_by_key/2` | 3226 / 120 | 31,683 | 2,036,574 | 1.56% |
| 23 | `dl7_checker:bind_diagnostics/3` | 4 / 4 | 31,052 | 2,011,004 | 1.54% |
| 24 | `dl7_checker:resolve_edges/6` | 1046 / 4 | 28,935 | 2,018,298 | 1.43% |
| 25 | `dl7_lowerer:lower_declarations/4` | 6 / 6 | 28,390 | 2,011,070 | 1.41% |
| 26 | `dl7_checker:resolve_call/5` | 906 / 906 | 19,115 | 2,040,770 | 0.94% |
| 27 | `dl7_evaluator:worklist_loop/5` | 304 / 4 | 17,272 | 2,013,104 | 0.86% |
| 28 | `dl7_checker:dense_index_diagnostics/4` | 4 / 4 | 16,250 | 2,011,004 | 0.81% |
| 29 | `dl7_lowerer:expression_callable/4` | 870 / 870 | 9,701 | 2,039,582 | 0.48% |
| 30 | `dl7_evaluator:demand_cone_static_indexes/4` | 3 / 3 | 6,280 | 2,010,971 | 0.31% |
| 31 | `dl7_checker:owner_edge_count_index/2` | 4 / 4 | 4,771 | 2,011,004 | 0.24% |
| 32 | `assoc:list_to_assoc/2` | 52 / 52 | 3,602 | 2,012,588 | 0.18% |
| 33 | `dl7_lowerer:lower_expression/7` | 2740 / 2738 | 1,748 | 2,101,240 | 0.08% |
| 34 | `system:sort/2` | 6437 / 6437 | 0 | 2,223,293 | 0.00% |
| 35 | `system:msort/2` | 892 / 892 | 0 | 2,040,308 | 0.00% |
| 36 | `system:keysort/2` | 159 / 159 | 0 | 2,016,119 | 0.00% |

The three sort rows count C-builtin invocations. Their clamped inference readings
do **not** mean sorting takes zero CPU: SWI does not charge one inference for each
C-level comparison or list element. No CPU attribution by sort call site was
established in this pass.

Additional broad-screen zeros: `materialize_syntax/4`, `accepted_rows/2`,
`ord_list_to_assoc/2`, and the compatibility wrapper `demand_cone_rules/6`.
The evaluator now calls `demand_cone_rules_indexed/4` directly, with three static
index builds for three evaluations. Syntax macro dispatch `expand_syntax/5`
ran once; plain reader expansion and syntax reification are distinct rows above.
`lower_expression/7` is exercised primarily for variable/literal arguments on
this fixture; its low own inference reading does not cover all executable
lowering, which costs 116,662 inclusive inferences.

### Overlap and intended modes

Do not sum this table into a claimed cost total. These observed paths overlap:

```text
evaluate(+Rules,+Seeds,-Closure,-Diagnostics)
  demand_cone_static_indexes(+Strata,+Rules,+Dependencies,-Indexes)
  demand_cone_rules_indexed(+Level,+Indexes,+CurrentRules,-Rules)
    demand_cone_worklist/8
  install_evaluation(+Id,+Rules,+Seeds,+LowerRows,-ClauseRefs)
    install_lower_rows(+Rows,+Id,-Refs)
      install_shared_lower_rows(+Rows,+StoreId,-Refs)
        index_argument_hashes(+Args,-H1,-H2,-H3,-H4)
  collect_closure(+Id,+ResultRelations,+LowerRows,-Closure)
    proves(+Id,?Call)

check_datalog(+Basement,+Origins,-Checked,-Diagnostics)
  bind_diagnostics(+Edges,+Origins,-Diagnostics)
    dense_index_diagnostics(+Edges,+AllEdges,+Origins,-Diagnostics)
      owner_edge_count_index(+Edges,-Counts)
  resolve_rules(+Rules,+Index,+Edges,+Nodes,+Relations,+Origins,-Resolved,-Diagnostics)
    check_goal_sequence_failures/7
    head_safety_diagnostics/5

check_resolved_rules(+Relations,+Rules,-Depends,-Strata,-Diagnostics)
  check_goal_sequence_failures/7
  head_safety_diagnostics/5
  stratify_rules/3

lower_datalog_mode(+Policy,+Unit,+ImportedEnvironment,-Program,-Origins,-Diagnostics)
  lower_declarations/4
  lower_executables/6
    lower_expression(+Node,+Owner,+Environment,-Value,-Goals,-Origins,-Diagnostics)

reader/embedder paths
  read_dl7(+Path,+Text,-Forms,-SourceRows,-Diagnostics)
  expand_dl7/6
  reify_syntax/4
```

Shared rule checking contributes under both public checker entry points.
`stratify_rules/3` runs from checker paths; evaluation also uses the underlying
dependency-aware stratification entry point. Thus its row is not a complete
total for every stratification invocation. Grouping/index helpers are shared
across callers and cannot be charged wholesale to a single subsystem.

## Emitted trace phase split and its current attribution defect

The unwrapped `DL7_TRACE=off` run still emitted the normal phase summary.
In ledger completion order its inference fields were:

| Phase occurrence | Recorded inferences |
| --- | ---: |
| read | 3,335 |
| expand | 565,480 |
| first lower | 10,051 |
| first comptime | 64,047 |
| first check | 81,890 |
| second lower | 84,004 |
| second comptime | 1,055,141 |
| second check | 1,201,235 |
| outer trace total | 2,010,416 |

These phase intervals overlap. In particular the check entries include downstream
comptime. The total is below the outer compile counter because summary/finalizer
work lies outside its measurement.

Source cause:
[debug_checker_input/2](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:101)
has a structured-input clause and an overlapping fallback at line 118. The first
clause leaves a choicepoint.
[run_compile_phase/3](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1b_compiler_tracer.pl:443)
uses `call_cleanup/2`; its finalizer waits for alternatives to be exhausted or
discarded. The caller proceeds into comptime before discarding the check's
alternative. The resulting phase intervals therefore include that continuation.

A temporary runtime wrapper `once(WrappedDebugInput)`, without editing source,
isolated this cause:

| Phase occurrence | With temporary debug-helper commit |
| --- | ---: |
| first check | 17,740 |
| first comptime | 64,050 |
| second check | 145,992 |
| second comptime | 1,055,144 |
| outer compile | 2,010,872 |

Check then completed before comptime in both pairs. Exact output hash remained
unchanged. This probe demonstrates corrected attribution and determinism, not a
whole-compile inference improvement: its counter is seven inferences above the
unwrapped observation due to the temporary wrapper.

A separate fresh `DL7_TRACE=steps` run retained the exact hash and recorded
2,075,221 outer compile inferences, including step instrumentation/output.
Selected step counters:

| Step category | Calls | Inclusive step inferences |
| --- | ---: | ---: |
| evaluator install | 15 | 112,210 |
| evaluator collect | 15 | 174,750 |
| evaluator cleanup | 15 | 29,175 |
| prelude cache miss | 1 | 562,049 |
| macrotime program cache miss | 1 | 153,221 |
| macro program evaluation round | 1 | 10,148 |
| main evaluation round 1 | 1 | 269,357 |
| main evaluation round 2 | 1 | 270,075 |

Install/collect/cleanup lie inside evaluation rounds; rounds lie inside the
compiler cache step. These rows also must not be added across hierarchy levels.
The prelude cache-miss step includes parsing, reader expansion, and reification.
It does not establish repeated parsing of an unchanged cached unit.

## Smallest semantics-neutral next patch

**Commit the structured case in the checker debug-input helper.**

- Source:
  [1_checker.pl:104](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:104).
- Mode: `debug_checker_input(+Basement,+Origins)`; in the valid compiler path
  Basement is a ground `basement_program(root_graph(...),datalog_program(...))`.
- Change: one `!` immediately after that structured clause head. The existing
  fallback remains for other input shapes.
- Repeated operation: the fallback is a second successful debug-helper proof,
  allowing all subsequent checker work to rerun on backtracking and postponing
  the phase finalizer.
- Expected bound: one successful debug-helper branch per structured input instead
  of two; check-phase lifetime ends at its own goal completion. The measured
  runtime wrapper shows the trace split changes while canonical compiler data
  remains exact.
- Preserved contracts: checked terms, diagnostics, rule order, binding, strata,
  macro/comptime boundaries, and emitted rows. Raw duplicate proof multiplicity
  is intentionally removed from a helper whose purpose is side-effect-only debug
  recording.
- Expected performance effect on ordinary first-result compilation: no material
  reduction established. The immediate result is a usable phase attribution
  boundary and removal of duplicate checker solutions.

Exact coverage to add before landing this one-line patch:

1. In `v7/test/1_entrypoints.test.pl`, enumerate the empty valid basement checker
   result with `findall/3`; assert the exact result list contains one
   `Checked-Diagnostics` pair, with `Diagnostics=[]`. Exercise trace off and debug,
   restoring environment/state afterward. The current source produces two pairs.
2. In `v7/test/3_compiler_trace.test.pl`, within an outer compile scope, call
   `run_compile_phase(check,check_datalog(...),Measurement)`, then immediately
   inspect `collected_compile_phases/1` before executing any later continuation.
   Assert exactly one check phase is already recorded and Measurement is ground.
   Do not put `once/1` around the phase in the test, as that would mask the bug.
3. Preserve current exact-output fixtures and run the focused existing tests
   `debug_phase_events_preserve_failure_and_exception`,
   `debug_exception_propagates_and_clears_state`, and
   `debug_trace_preserves_compile_output`, each within the V7 per-case budget.
4. Re-run this cold nearest-shadow probe and the existing performance gate with
   unchanged row/diagnostic checkpoints. Compare canonical output, not phase
   inference byte identity, since the phase lifetimes are deliberately corrected.

No tests were edited or added by this audit. The recommendations above are
proposed coverage, not a claim that newly written tests ran.

## Separately measured residual queue copying

The indexed demand-cone worklist still executes
[append at line 311](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:311)
when `NewRelations=[]`. This is distinct from the stratification worklist, whose
empty-enqueue guard is already present at line 915.

A second source-preserving runtime wrapper counted exact queue shapes during
nearest-shadow: **370 nonempty queue pops, 252 empty-discovery appends, 2,459
remaining queue cells copied in those empty cases, maximum remaining length 27**.
Exact output hash was unchanged. For N queued leaves this empty-append work is
N(N-1)/2 copied cells. Reusing Queue0 on empty discovery would remove those copies
without changing FIFO order; this audit did not implement or claim a measured
whole-compile gain for that change. The one-line debug-helper patch above is the
selected next action.

## Reproduction and verification scope

Temporary harness: `/private/tmp/dl7_current_head_profile.pl`.

```bash
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- baseline
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- single dl7_checker check_datalog 4
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- steps
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- debug_commit
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- cone
```

The `single` command was repeated serially for each extended-table predicate.
The `wrapped` mode installed all candidates and was used only to screen.
Normal source and test files were unchanged; runtime wrappers expired with each
process. No full test suite or build was run. This work adds, changes, or removes
no CI coverage. No commit or push was performed. Unrelated dirty work was preserved.

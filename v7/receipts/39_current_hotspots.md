# 39. Current committed-head hotspots

Date: 2026-09-11. Native Astra, read-only.
HEAD throughout all probes: `f6b46dcf204701ca48f43f99f1c7784deacf2735`.
Includes accepted parser commit `62a25b380` and checker cut `00f6f80d4`.
Fixture: `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`.

## Fresh result and scope

Two fresh unwrapped processes, compiler caches cleared, `DL7_TRACE=off`:

| Measurement | First | Final confirmation |
| --- | ---: | ---: |
| Whole compile inferences | 1,994,061 | 1,994,061 |
| Wall | 449.980 ms | 424.458 ms |
| Published rows | 810 | 810 |
| Diagnostics | `[]` | `[]` |

Canonical `write_canonical(result(Rows,Runtime,Diagnostics))` SHA-256:

```text
f42d4b6a73ce4cdd97594ff9710377c499a8c78e2ff8987517793cbbca29a99a
```

All **81 isolated single-target passes** and the separate safety-head attribution
probe produced this same hash, 810 rows, and empty diagnostics. Every process had
an external `timeout 15s`; no SWI processes overlapped. Source and tests were
unchanged at start/end. These are new measurements, not copied receipt-34 counts.
Identical costs for unchanged predicates are fresh observations.

The SWI performance skill determined canonical parity, serial bounded execution,
supported-mode inspection, and inference/wall separation. No source/test edits,
implementation, CI run, commit, or push occurred. This receipt adds no CI coverage.

## Top 10 inclusive measured targets

Each row is its own fresh process, wrapping only the named predicate. Calls count
all entries; outer calls are those without an active recursive ancestor of that
same predicate. The cost includes descendants once within each outer invocation.
`call_time/3` records nondeterministic answer/failure segments without charging
unrelated caller continuation. Share uses the exact instrumented compile
denominator in that row, never another pass's total.

**These are raw instrumented inclusive costs**, ranked among the 81 named targets,
not an exhaustive predicate census. Recursive wrapper work is visible inside
ancestor timing. In particular, token scanning's raw rank 5 is materially inflated;
the explicit overhead accounting below prevents interpreting 251,051 as its
unwrapped cost. Do not sum any table of inclusive parent/child rows.

| Rank | Predicate | Calls / outer | Inclusive inferences | Own-pass compile inferences | Share |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | `dl7_evaluator:evaluate/4` | 3 / 3 | 489,188 | 1,994,167 | 24.53% |
| 2 | `dl7_parser:read_dl7/5` | 3 / 3 | 362,042 | 1,994,167 | 18.16% |
| 3 | `dl7_checker:check_resolved_rules/5` | 5 / 5 | 279,671 | 1,994,233 | 14.02% |
| 4 | `dl7_checker:check_datalog/4` | 4 / 4 | 276,176 | 1,994,200 | 13.85% |
| 5 | `dl7_parser:take_token/5` | 22,246 / 3,179 | 251,051 | 2,232,444 | 11.25% |
| 6 | `dl7_checker:resolved_rule_diagnostics/3` | 389 / 5 | 219,224 | 1,996,921 | 10.98% |
| 7 | `dl7_lowerer:lower_datalog_mode/6` | 6 / 6 | 178,388 | 1,994,266 | 8.95% |
| 8 | `dl7_evaluator:collect_closure/4` | 15 / 15 | 173,421 | 1,994,563 | 8.69% |
| 9 | `dl7_evaluator:proves/2` | 3,806 / 167 | 170,457 | 2,088,567 | 8.16% |
| 10 | `dl7_checker:resolve_rules/8` | 268 / 4 | 160,632 | 1,996,048 | 8.05% |

### Recursive instrumentation cost

For 77 nonzero targets other than the two nondeterministic evaluator targets,
observed whole-compile overhead exactly obeys:

```text
OwnPassTotal - 1,994,061
  = 33 * OuterCalls + 7 * (Calls - OuterCalls) + 7
```

There are 79 nonzero targets: `lower_arguments/5` and expander
`source_for_node/3` have zero calls. The two exceptions,
`proves/2` and `evaluation_lower/3`, incur additional answer/redo
instrumentation; this deterministic-call formula must not be applied to them.

For `take_token/5`, 19,067 recursive entries contribute 133,469 wrapper
inferences inside the outer token scans. Subtracting that known recursive
component from 251,051 gives **117,582**, a limited corrected estimate, not a
separately timed unwrapped predicate counter. Likewise:

| Recursive target | Raw inclusive | Known nested-wrapper component | After subtracting that component |
| --- | ---: | ---: | ---: |
| parser `take_token/5` | 251,051 | 133,469 | 117,582 |
| parser `skip_layout/4` | 100,295 | 46,788 | 53,507 |
| parser `skip_comment/4` | 54,487 | 34,566 | 19,921 |
| checker `resolved_rule_diagnostics/3` | 219,224 | 2,688 | 216,536 |
| checker `resolve_rules/8` | 160,632 | 1,848 | 158,784 |
| checker `add_variables/3` | 88,192 | 60,578 | 27,614 |

Outer timing corrections/clamping still limit very small readings. Foreign
`sort/2`, `msort/2`, and `keysort/2` do not charge one Prolog inference per
C-level comparison or list cell; their zero readings do not imply zero CPU.
The unwrapped compile denominator remains 1,994,061.

## Narrow operations and actionable repetition

Here “narrow” means a concrete internal operation, not necessarily a predicate
with no callees. Each measurement remains inclusive of its own descendants.

| Operation | Fresh calls | Fresh inclusive inferences | Concrete source and repeated operation |
| --- | ---: | ---: | --- |
| Checker head-variable collection | 1,296 | 133,902 | `1_checker.pl:805`, `head_variables(+Call,-Identities)`: per argument, collect nested var identities, then sort |
| Checker nested argument-variable collection | 4,648 | 118,406 | `1_checker.pl:766`, `argument_variables(+Argument,-Identities)`: enumerate all `sub_term/2` descendants, select `var(Identity)`, then sort |
| Parser identifier validation | 2,471 | 90,247 | `0_parser.pl:317`, `valid_identifier_codes(+Codes)`: classify first character and every remaining character |
| Evaluator argument hash construction | 3,782 | 81,173 | `0_evaluator.pl:1061`, `index_argument_hashes(+Args,-H1,-H2,-H3,-H4)`: four nth-position, groundness, and hash checks |
| Checker goal-variable collection | 4,629 | 69,265 | `1_checker.pl:879`, `goal_variables(+Goal,-Identities)`: new findall bag of top-level argument vars |
| Graph transitive closure for strict cycles | 4 | 63,207 | `0_evaluator.pl:789` calls `ugraphs:transitive_closure(+Graph,-Closure)` before strict-edge checks |
| Parser atom validation | 1,862 | 61,659 | `0_parser.pl:306`, `valid_atom_codes(+Codes)`: special-token membership, trailing-colon split attempt, identifier validation |
| Lowerer call-argument normalization | 868 | 60,946 | `0_lowerer.pl:1185`: classify named/positional args, assign slots, lower values, fill absent slots |
| Syntax source lookup | 4,367 | 43,671 | `1a_syntax_grapher.pl:137`: collect indexed source rows into a bag to distinguish zero/one/multiple origins |

Head collection includes argument collection; atom validation includes identifier
validation; normalization includes its slot helpers. Do not add these rows.
Identifier validation is shared by atom, variable, and symbol paths, explaining
why its whole-compile counter exceeds the atom-only caller.

### Smallest measured reuse candidate: already computed head variables

Both rule-checking paths compute `HeadVariables` before mode checking and then
call safety checking, which recomputes the same list:

- [resolved_rule_diagnostic/3](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:194)
  computes it at line 198, then calls safety checking at line 202.
- [resolve_rules/8](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:582)
  computes it at line 592, then calls safety checking at line 596.
- [head_safety_diagnostics/5](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:865)
  recomputes through `head_variables/2` at line 867.

A separate scope-filtered wrapper timed `head_variables/2` only while inside
`head_safety_diagnostics/5`: **648 calls, 66,570 inclusive inferences**.
Whole instrumented compile was 2,023,228; output hash remained exact.
This isolates the second collection, rather than assuming half of the overall
head-variable total.

Proposed physical change: retain the existing `head_safety_diagnostics/5`
wrapper for qualified callers, introduce an internal worker accepting the already
computed sorted identities, and use it from the two rule-checking paths.
Keep `argument_variables/2`, body-variable collection, origin lookup,
`unsafe_vars/4`, diagnostic ordering, and all supported nested-var semantics
unchanged. On this fixture the target is 648 avoided repeated collections,
with an observed cost window of 66,570, about 3.34% of the unwrapped compile.
That is a candidate bound, not a measured implementation saving.

This requires no proposed language-contract change. No implementation was
authorized or performed by this profiling task. Regression checks would compare
exact diagnostics for safe/unsafe head vars, repeated/nested identities, negative
and constructive goals, plus current generated-rule, nearest-shadow, and partial
outputs. Do not replace nested `sub_term/2` discovery with top-level-only
matching without separately proving the supported representation contract.

Other narrowed costs above need their own repeated-input/cardinality evidence
before selecting a rewrite. Token scanning currently visits each token character;
its raw wrapper-inflated counter alone does not establish redundant scanning.
Syntax child-row assembly still copies child rows through append, but no
source-preserving replacement was evaluated here.

## Parent overlap map

```text
evaluate/4
  static cone indexes + per-stratum cone selection
  install_evaluation/5
    install_shared_lower_rows/3
      install_lower_rows_plain/3
        index_argument_hashes/5
  collect_closure/4
    proves/2
      evaluation_lower/3
        index_argument_hashes/5

check_datalog/4
  bind_diagnostics/3 -> dense_index_diagnostics/4
  resolve_rules/8
    head_variables/2 -> argument_variables/2
    check_goal_sequence_failures/7 -> check_goal_transition/6 -> check_goal/4
    head_safety_diagnostics/5 -> head_variables/2 again

check_resolved_rules/5
  resolved_rule_diagnostics/3
    same head/mode/safety operations
  stratify_rules/3 -> strict_cycle_diagnostics/3 -> transitive_closure/2

lower_datalog_mode/6 -> lower_executables/6 -> lower_rule/7
  lower_goals/4 -> lower_call/4 -> normalize_call_arguments/8
    classify_argument_nodes/4 + assign_argument_slots/5
    lower_assigned_arguments/8 -> lower_expression/7
    fill_slot_arguments/4

read_dl7/5
  skip_layout/4 -> skip_comment/4
  take_token/5 -> term_delimiter/1 + advance/3
  finish_token/9 -> integer_codes/1 + valid_atom_codes/1
  finish_variable/10 -> valid_identifier_codes/1 + variable_identity/6

reify_syntax/4
  install_source_row_index/1
  reify_frontier/4 -> reify_node/3 <-> reify_children/5
    source_row_result/3
```

`proves(+EvaluationId,?Call)` remains nondeterministic/variant-tabled; collector
roots bind the relation. Hashes narrow ground arguments and exact stored-term
unification remains mandatory. Other concrete operation modes above consume
ground compiler data and produce trailing outputs.

## Current trace-off phase ledger

Both unwrapped runs recorded these exact phase inference counts. Check now
finishes before the following comptime interval.

| Phase occurrence, execution order | Inferences |
| --- | ---: |
| read | 3,335 |
| expand | 550,162 |
| macro lower | 10,051 |
| macro check | 17,737 |
| macro comptime | 64,047 |
| source lower | 84,004 |
| source check | 145,989 |
| source comptime | 1,055,141 |
| outer trace total | 1,993,612 |
| outer compile counter | 1,994,061 |

The source comptime interval includes rule rechecking, evaluation rounds, final
lowering/checking, and refreeze coordination. It cannot be added to those child
predicate costs. Trace total and outer compile counter have different finalizer
boundaries.

## Remaining isolated target measurements

Same definitions and exact per-row denominators as the top-10 table. Ordered by
raw inclusive cost. Together the two tables contain all 81 tested targets.

| Predicate | Calls / outer | Inclusive inferences | Own-pass compile inferences | Share |
| --- | ---: | ---: | ---: | ---: |
| `dl7_syntax_grapher:reify_syntax/4` | 4 / 4 | 142,676 | 1,994,200 | 7.15% |
| `dl7_syntax_grapher:reify_children/5` | 5,254 / 259 | 141,397 | 2,037,580 | 6.94% |
| `dl7_syntax_grapher:reify_node/3` | 4,367 / 259 | 139,333 | 2,031,371 | 6.86% |
| `dl7_checker:check_goal_sequence_failures/7` | 2,224 / 648 | 138,573 | 2,026,484 | 6.84% |
| `dl7_checker:head_variables/2` | 1,296 / 1,296 | 133,902 | 2,036,836 | 6.57% |
| `dl7_syntax_grapher:reify_frontier/4` | 263 / 4 | 131,149 | 1,996,013 | 6.57% |
| `dl7_checker:check_goal_transition/6` | 1,576 / 1,576 | 123,461 | 2,046,076 | 6.03% |
| `dl7_checker:head_safety_diagnostics/5` | 648 / 648 | 120,042 | 2,015,452 | 5.96% |
| `dl7_checker:argument_variables/2` | 4,648 / 4,648 | 118,406 | 2,147,452 | 5.51% |
| `dl7_lowerer:lower_executables/6` | 6 / 6 | 116,662 | 1,994,266 | 5.85% |
| `dl7_lowerer:lower_rule/7` | 262 / 262 | 112,644 | 2,002,714 | 5.62% |
| `dl7_evaluator:stratify_rules/3` | 9 / 9 | 112,022 | 1,994,365 | 5.62% |
| `dl7_evaluator:install_evaluation/5` | 15 / 15 | 110,881 | 1,994,563 | 5.56% |
| `dl7_evaluator:install_shared_lower_rows/3` | 15 / 15 | 100,336 | 1,994,563 | 5.03% |
| `dl7_parser:skip_layout/4` | 12,163 / 5,479 | 100,295 | 2,221,663 | 4.51% |
| `dl7_expander:expand_dl7/6` | 3 / 3 | 97,269 | 1,994,167 | 4.88% |
| `dl7_parser:valid_identifier_codes/1` | 2,471 / 2,471 | 90,247 | 2,075,611 | 4.35% |
| `dl7_evaluator:install_lower_rows_plain/3` | 2,694 / 15 | 89,395 | 2,013,316 | 4.44% |
| `dl7_checker:add_variables/3` | 11,642 / 2,988 | 88,192 | 2,153,250 | 4.10% |
| `dl7_lowerer:lower_goals/4` | 902 / 262 | 83,212 | 2,007,194 | 4.15% |
| `dl7_parser:finish_token/9` | 1,889 / 1,889 | 81,709 | 2,056,405 | 3.97% |
| `dl7_evaluator:index_argument_hashes/5` | 3,782 / 3,782 | 81,173 | 2,118,874 | 3.83% |
| `dl7_checker:check_goal/4` | 1,576 / 1,576 | 76,864 | 2,046,076 | 3.76% |
| `dl7_expander:node_tree/2` | 12,463 / 4,339 | 75,643 | 2,194,123 | 3.45% |
| `dl7_lowerer:lower_call/4` | 640 / 640 | 70,962 | 2,015,188 | 3.52% |
| `dl7_evaluator:strict_cycle_diagnostics/3` | 4 / 4 | 69,436 | 1,994,200 | 3.48% |
| `dl7_checker:goal_variables/2` | 4,629 / 4,629 | 69,265 | 2,146,825 | 3.23% |
| `dl7_evaluator:validate_functional_rows/3` | 4 / 4 | 64,165 | 1,994,200 | 3.22% |
| `ugraphs:transitive_closure/2` | 4 / 4 | 63,207 | 1,994,200 | 3.17% |
| `dl7_parser:valid_atom_codes/1` | 1,862 / 1,862 | 61,659 | 2,055,514 | 3.00% |
| `dl7_lowerer:normalize_call_arguments/8` | 868 / 868 | 60,946 | 2,022,712 | 3.01% |
| `dl7_parser:skip_comment/4` | 5,022 / 84 | 54,487 | 2,031,406 | 2.68% |
| `dl7_parser:finish_variable/10` | 1,290 / 1,290 | 53,170 | 2,036,638 | 2.61% |
| `dl7_syntax_grapher:source_row_result/3` | 4,367 / 4,367 | 43,671 | 2,138,179 | 2.04% |
| `dl7_parser:advance/3` | 34,480 / 34,480 | 34,481 | 3,131,908 | 1.10% |
| `dl7_parser:identifier_rest_code/1` | 15,757 / 15,757 | 34,138 | 2,514,049 | 1.36% |
| `dl7_evaluator:demand_cone_rules_indexed/4` | 15 / 15 | 32,697 | 1,994,563 | 1.64% |
| `dl7_lowerer:lower_assigned_arguments/8` | 3,478 / 868 | 32,587 | 2,040,982 | 1.60% |
| `pairs:group_pairs_by_key/2` | 3,226 / 120 | 31,683 | 2,019,770 | 1.57% |
| `dl7_checker:bind_diagnostics/3` | 4 / 4 | 31,052 | 1,994,200 | 1.56% |
| `dl7_lowerer:classify_argument_nodes/4` | 3,482 / 870 | 30,143 | 2,041,062 | 1.48% |
| `dl7_evaluator:rule_dependencies/2` | 912 / 12 | 29,685 | 2,000,764 | 1.48% |
| `dl7_checker:resolve_edges/6` | 1,046 / 4 | 28,935 | 2,001,494 | 1.45% |
| `dl7_evaluator:evaluation_lower/3` | 1,103 / 1,103 | 28,848 | 2,053,700 | 1.40% |
| `dl7_lowerer:lower_declarations/4` | 6 / 6 | 28,390 | 1,994,266 | 1.42% |
| `dl7_checker:resolve_name/6` | 2,050 / 1,476 | 25,617 | 2,046,794 | 1.25% |
| `dl7_lowerer:fill_slot_arguments/4` | 3,478 / 868 | 25,233 | 2,040,982 | 1.24% |
| `dl7_lowerer:scoped_reservation/5` | 2,070 / 1,502 | 24,635 | 2,047,610 | 1.20% |
| `dl7_lowerer:promote_deferred_aliases/9` | 6 / 6 | 23,604 | 1,994,266 | 1.18% |
| `dl7_lowerer:assign_argument_slots/5` | 870 / 870 | 20,894 | 2,022,778 | 1.03% |
| `dl7_checker:resolve_call/5` | 906 / 906 | 19,115 | 2,023,966 | 0.94% |
| `dl7_parser:term_delimiter/1` | 22,246 / 22,246 | 19,068 | 2,728,186 | 0.70% |
| `dl7_evaluator:worklist_loop/5` | 304 / 4 | 17,272 | 1,996,300 | 0.87% |
| `dl7_checker:dense_index_diagnostics/4` | 4 / 4 | 16,250 | 1,994,200 | 0.81% |
| `dl7_checker:body_refs/3` | 2,224 / 648 | 15,113 | 2,026,484 | 0.75% |
| `dl7_syntax_grapher:install_source_row_index/1` | 4 / 4 | 13,272 | 1,994,200 | 0.67% |
| `dl7_expander:rewrite_fixpoint/7` | 4,339 / 4,339 | 9,815 | 2,137,255 | 0.46% |
| `dl7_parser:integer_codes/1` | 1,889 / 1,889 | 9,479 | 2,056,405 | 0.46% |
| `dl7_lowerer:lower_derived_bind_rules/5` | 824 / 6 | 8,107 | 1,999,992 | 0.41% |
| `dl7_evaluator:demand_cone_static_indexes/4` | 3 / 3 | 6,280 | 1,994,167 | 0.31% |
| `assoc:list_to_assoc/2` | 52 / 52 | 3,602 | 1,995,784 | 0.18% |
| `dl7_parser:variable_identity/6` | 1,290 / 1,290 | 1,946 | 2,036,638 | 0.10% |
| `dl7_lowerer:lower_expression/7` | 2,740 / 2,738 | 1,748 | 2,084,436 | 0.08% |
| `dl7_parser:read_string_codes/3` | 120 / 23 | 1,405 | 1,995,506 | 0.07% |
| `dl7_syntax_grapher:syntax_graph_result/3` | 4 / 4 | 1 | 1,994,200 | 0.00% |
| `dl7_lowerer:lower_arguments/5` | 0 / 0 | 0 | 1,994,061 | 0.00% |
| `dl7_parser:source_row/5` | 4,339 / 4,339 | 0 | 2,137,255 | 0.00% |
| `dl7_expander:source_for_node/3` | 0 / 0 | 0 | 1,994,061 | 0.00% |
| `system:sort/2` | 6,437 / 6,437 | 0 | 2,206,489 | 0.00% |
| `system:msort/2` | 892 / 892 | 0 | 2,023,504 | 0.00% |
| `system:keysort/2` | 159 / 159 | 0 | 1,999,315 | 0.00% |

## Reproduction

Reusable temporary harness:
`/private/tmp/dl7_current_head_profile.pl`, read and reused without editing.

```bash
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- baseline
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- single dl7_parser read_dl7 5
timeout 15s swipl -q -s /private/tmp/dl7_current_head_profile.pl -g main -t halt -- single dl7_checker argument_variables 2
```

The single-target command ran separately for every table row. One small reflection
query confirmed private predicate arities before probing. The safety-head probe
used runtime wrappers: mark entry/exit of `head_safety_diagnostics/5`, and
record `head_variables/2` only while that scope marker is active. Those wrappers
expired with the process. No persistent compiler state or source edits were made.

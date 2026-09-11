# 42. DL7 Prolog arena and relational data-model inventory

Date: 2026-09-11. Read-only compiler investigation on branch HEAD
`95983d120df81d368c7eeac8f3b1273081525f97`. No compiler, test, benchmark,
workflow, skill, or application file was edited. No build, commit, push, merge,
or delegation. Temporary probes live under `/private/tmp`.

## TOC

- [Scope and method](#scope-and-method)
- [1. Canonical entities crossing compiler phases](#1-canonical-entities-crossing-compiler-phases)
- [2. Lookups over entities](#2-lookups-over-entities)
- [3. Repeated derived values](#3-repeated-derived-values)
- [4. Identities and interning](#4-identities-and-interning)
- [5. Proposed compiler_arena schema](#5-proposed-compiler_arena-schema)
- [6. Ranked migration candidates](#6-ranked-migration-candidates)
- [7. Phase data flow and ownership](#7-phase-data-flow-and-ownership)
- [Probe receipts and reproduction](#probe-receipts-and-reproduction)
- [Measured versus proposed](#measured-versus-proposed)

## Scope and method

Two fixtures are measured: `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`
(NS) and `v7/test/fixtures/2_partial.dl7` (2P). Each probe is one fresh SWI
process under `timeout 20`, and each completes under 3 s except the two
`library(prolog_profile)` call-count probes, which are marked and excluded from
inference claims.

Probes:

| Probe | Harness | Fact produced |
| --- | --- | --- |
| P1 reader | `/private/tmp/dl7_probe_reader.pl` | unit, reader node, source row, syntax row counts |
| P2 debug | `/private/tmp/dl7_probe_debug.pl` | `COMPILE-TRACE-DEBUG` phase cardinalities |
| P3 entities | `/private/tmp/dl7_probe_entities.pl` | closure fact, distinct ref/owner, arity histogram |
| P4 wrap | `/private/tmp/dl7_probe_wrap.pl` | lookup call counts, scanned-item totals, max/mean |
| P5 scope | `/private/tmp/dl7_probe_scope.pl` | per-`evaluate/4` rule/seed install redundancy |
| P6 profile | `v7/bench/1_compiler_profile.pl` | duplicate occurrence report per category |
| R39 | `v7/receipts/39_current_hotspots.md` | inclusive predicate and C-builtin call counts |

Scope labels in P2: scope 2 is the macrotime bootstrap compile; scope 3 is the
main source compile. Cardinalities below are scope 3 unless stated.

## 1. Canonical entities crossing compiler phases

`entity_count = 20`. Cardinalities are P1/P2 measured; `->` means the term is
rewritten at that boundary.

| # | Constructor | Fields | Producer predicate | Consumer predicate(s) | NS card | 2P card |
| ---: | --- | --- | --- | --- | ---: | ---: |
| 1 | `dl7_unit/5` | Origin, Digest, Forms, SourceRows, ExpansionRows | `dl7_text_unit/6` `2_embedder.pl:38` | `reify_syntax/4`, `expand_dl7/6`, `materialize_syntax/4`, lowerer | 1 source, 1 prelude, 1 macrotime | same |
| 2 | `node/2` reader | NodeId, Payload = atom/literal/variable/form | `read_dl7/5` `0_parser.pl` | `reify_syntax/4`, `expand_dl7/6`, `materialize_syntax/4` | 28 src / 3929 prelude | 192 src / 3929 prelude |
| 3 | `source/8` | NodeId, Path, StartOffset, EndOffset, StartLine, StartColumn, EndLine, EndColumn | `read_dl7/5` | `install_source_row_index/1`, `source_for_node/3`, `source_result/4` | 28 | 192 |
| 4 | syntax rows | `node/1`, `syntax_atom/2`, `syntax_literal/2`, `syntax_variable/3`, `syntax_form/1`, `syntax_frontier/2`, `':'/4` | `reify_syntax/4` `1a_syntax_grapher.pl:31` | `materialize_syntax/4` | 112 rows | 768 rows |
| 5 | `expansion/4` | InputNodeId, MacroIdentity, Wave, NodeId | `mint_tree/10` `1_expander.pl:176` | none (write-only audit finding) | 0 | 0 |
| 6 | `pending_edge/4` | Owner, Name, Target, Index | lowerer | `bind_diagnostics/3`, `resolve_edges/6`, `resolve_name/6` | 484 | 501 |
| 7 | `module_basement/2` | Owner, `basement_program/2` | `lower_units_*` | `merge_module_basements/4` | 2 units | 2 units |
| 8 | `basement_program/2` | `root_graph/2`, `datalog_program/3` | `merge_module_basements/4` | `check_datalog/4` | 1 | 1 |
| 9 | `relation/3` | Ref/Target, Arity, KeySets | lowerer; `relations_refs/2`, `kernel_relation_rows/1`, `add_generated_relations/3` | `resolved_call_diagnostics/3`, `strata_rows/3`, `validate_functional_rows/3` | 114 -> 134 | 120 -> 140 |
| 10 | `rule/2` | Head `call/2`, Body list `checked_goal/2` | `lower_rule/7`; `resolve_rules/8` | `resolved_rule_diagnostics/3`, evaluator | 120 | 126 |
| 11 | `checked_goal/2` | Polarity, `call/2` | `resolve_goals/8` | `proves_body/2` | in rule bodies | in rule bodies |
| 12 | `call/2` | Ref, Arguments | lowerer, evaluator | checker, evaluator, emitters | 810 closure rows | 910 closure rows |
| 13 | `ref/1` | `kernel/1`, `owner/2`, `primitive/1`, `application/2` | lowerer, checker | all phases | 7 distinct in facts | 40 distinct |
| 14 | `dependency/5` | HeadRel, BodyRel, Polarity, Gap, Cause | `rule_dependencies/2` | `stratify_rules/3`, `strict_cycle_diagnostics/3` | 292 | 309 |
| 15 | `stratum/2` | Relation, Level | `strata_rows/3` | `rule_at_level/3`, `seed_at_level/3` | 134 | 140 |
| 16 | `origin/2` | `edge/3`/`seed/1`/`rule/1`/`goal/2`, NodeId | lowerer | `edge_origin/5`, `seed_origin/3`, `rule_origin/3`, `goal_origin/4` | 1124 | 1177 |
| 17 | `diagnostic/3` | Phase, NodeId, Reason | every phase | compiler output | 0 | 0 |
| 18 | `checked_datalog/4` | `root_graph/2`, `datalog_program/3`, Depends, Strata | `check_datalog/4` | evaluator, compiler | 1 | 1 |
| 19 | `evaluation_rule/3` | EvaluationId, Relation, Rule | `install_rules/3` | `proves/2` | 220 asserts, 113 distinct per scope | 227 asserts, 120 distinct per scope |
| 20 | `evaluation_seed/3`, `evaluation_lower_index/7`, `evaluation_request/2` | Id, Relation, hashes/args, Row/Request | `install_seeds/3`, `install_lower_rows/3`, `record_evaluation_request/2` | `proves/2`, `evaluation_lower/3`, `collect_closure/4` | 1336 seeds max scope | 1407 seeds max scope |

Notable structural facts:

- Closure `call/2` rows are ground on both fixtures (P3 `DISTINCT_VARS 0`).
- `expansion/4` rows are produced by the expander and read by no consumer in
  these fixtures (P1 `expansion_rows=0` for NS and 2P, because neither source
  triggers a macro rewrite).
- `relation/3` count grows across the check boundary (114 -> 134 NS; 120 -> 140
  2P) as kernel and generated relations merge.

## 2. Lookups over entities

`lookup_count = 15`. `Scanned total/max/mean` is P4 measured over the list
argument when that argument is a list; `n/a` means the collection is not a list.
Calls are NS / 2P.

| # | Predicate | Supported / observed mode | Collection and mechanism | Calls NS / 2P | Scanned total / max / mean NS; 2P | Solutions/call | Choicepoints |
| ---: | --- | --- | --- | ---: | --- | --- | --- |
| 1 | `memberchk/2` | `+Elem,+List` det | linear list scan; Relations, Nodes, Edges, Origins | 94,438 / 198,296 | 13,545,263 / 1124 / 143.43 ; 18,997,001 / 1177 / 95.80 | <= 1 | none |
| 2 | `member/2` | `?Elem,+List` nondet | linear list scan | 11,622 / 20,610 | 87,361 / 3929 / 7.52 ; 279,449 / 4250 / 13.56 | many | leaves choicepoints (redo 33,637 / 58,878) |
| 3 | `get_assoc/3` | `+Key,+Assoc,?Value` det | AVL assoc | 5,421 / 12,270 | n/a | <= 1 | none (redo 0) |
| 4 | `put_assoc/4` | `+,+,-,+` det | AVL assoc | 675 / 2,068 | n/a | 1 | none |
| 5 | `list_to_assoc/2` | `+Pairs,-Assoc` det | AVL build | 52 / 134 | 971 / 85 / 18.67 ; 2,278 / 92 / 17.00 | 1 | none |
| 6 | `group_pairs_by_key/2` | `+Pairs,-Groups` det | keysort then group | 3,226 / 4,250 | 637,514 / 811 / 197.62 ; 772,796 / 924 / 181.83 | 1 | none |
| 7 | `ord_subtract/3`, `ord_union/3` | `+,+,-` det | ordered-set merge | 9,060 / 28,444 | 2,269,833 / 1341 / 250.53 ; 9,753,092 / 1494 / 342.89 | 1 | none |
| 8 | `term_hash/2` | `+Term,-Hash` det | hash of a ground argument | 10,553 / 42,646 | n/a | 1 | none |
| 9 | `evaluation_lower_index/7` | `Store,Rel,?,?,?,?,Row` nondet | dynamic clause index over 4 argument hashes | 1,103 / 5,480 | n/a | varies | dynamic index, nondet |
| 10 | `evaluation_lower/3` | wrapper over #9 | calls `index_argument_hashes/5` then #9 | 1,103 / 5,480 | n/a | varies | nondet |
| 11 | `proves/2` | `+EvaluationId,?Call` nondet | variant SLG table | 3,806 calls / 167 outer (R39) | n/a | varies | tabled |
| 12 | `sort/2` | `+List,-Sorted` det | C merge sort | 6,437 (R39) | n/a | 1 | none |
| 13 | `msort/2` | `+List,-Sorted` det | C merge sort | 892 (R39) | n/a | 1 | none |
| 14 | `keysort/2` | `+Pairs,-Sorted` det | C key sort | 159 (R39) | n/a | 1 | none |
| 15 | `sub_term/2` | `?Sub,+Term` nondet | structure decomposition inside `argument_variables/2` | 4,648 `argument_variables` calls (R39) | n/a | many | leaves choicepoints |

Mechanism census: `memberchk` and `member` are linear scans (P4); assoc reads and
writes are AVL (P4/R39); `group_pairs_by_key` and the `ord_*` family are the
index builders (P4); the evaluator lower store is a dynamic clause index keyed by
`relation + four argument hashes` (`0_evaluator.pl:1029`, `:1057`); `proves/2`
is variant-tabled (`0_evaluator.pl:50`); `sort/msort/keysort` are C builtins with
no Prolog inference charge (R39).

## 3. Repeated derived values

P6 duplicate occurrence report (`v7/bench/1_compiler_profile.pl`). Each category
keeps equal terms under its own semantic role, so percentages are within a
category. P5 splits rule and seed installs per `evaluate/4` scope.

| Value | Producer count | Unique inputs | Recomputation / duplicate count | Consumers | Estimated cost |
| --- | ---: | ---: | --- | --- | --- |
| stratified rule input | 12 NS / 22 2P | 4 / 5 | 8 dup (66.67%) / 17 dup (77.27%) | memoized `stratify_rules/3` | R20 memo bounds re-stratification |
| checker input basement | 9 NS / 14 2P | 5 / 7 | 4 dup (44.44%) / 7 dup (50.00%) | `check_datalog/4` | R39 `check_datalog/4` 276,176 incl |
| installed rule term | 452 NS / 1,599 2P | 238 / 850 | 214 dup NS / 749 dup 2P (P6 327/1,467 cross-round) | `proves/2` | P5 107 redundant asserts per main scope |
| installed seed term | 2,901 NS / 10,014 2P | 2,901 / 10,014 | 1,468 dup (50.60%) / 8,510 dup (84.98%) cross-round | `proves/2` | P5 redundant within scope = 0 |
| installed lower row | 2,679 NS / 10,257 2P | 1,341 / 1,494 | 1,338 dup (49.94%) / 8,763 dup (85.43%) cross-round | `collect_closure/4` | R24 shares rows within a scope; P5 confirms seed/lower dedup is per-scope |
| checker head/argument variables | 1,296 `head_variables` NS | 648 safety scopes after R40 | R40 removed 648 second collections | mode and safety checks | R40 -> -67,218 NS inferences |
| checker goal variables | 4,629 NS | 321 shapes | R41 removed 1,477 duplicate collections | mode transition | R41 -> -27,209 NS inferences |

Cross-round versus cross-stratum attribution: P5 is the split. Within one
`evaluate/4` scope the shared lower store already deduplicates lower rows and
seeds (redundant = 0), but rule clauses are re-asserted: 107 redundant asserts
per main scope on both fixtures. The P6 cross-round duplicates for seeds and
lower rows are intentional snapshot rebuilds and must not be treated as removable.

## 4. Identities and interning

| Identity kind | Exact shape | Producer | Equality/hash/sort site | Repeated work |
| --- | --- | --- | --- | --- |
| source node id | `reader_node(Origin, Index)`; `expansion_node(InputNodeId, MacroIdentity, Wave, Index)` | `read_dl7/5`; `mint_tree/10` | `memberchk`, `sort` | node ids reused across syntax/materialization |
| owner id | `owner(Origin, reader_node(...))`; `module(Origin)` | lowerer | `sort/2`, `memberchk/2` | 7 NS / 39 2P distinct owners in facts |
| semantic reference | `ref(kernel(Name))`, `ref(owner(...))`, `ref(primitive(Name))`, `ref(application(Constructor, Args))` | lowerer, checker | `sort/2`, `term_hash/2` | 7 NS / 40 2P distinct refs in closure |
| application term | `call(Ref, Arguments)` | lowerer, evaluator | `sort/2`, `==` in `proves` | all closure rows |
| argument vector | list of `const/1`/`var/1`/`ref/1`/`aggregate/2` | lowerer | `term_hash/2` per position | `index_argument_hashes/5` 3,782 calls / 81,173 incl NS |
| relation id | `relation(Ref, Arity, KeySets)` | lowerer, checker | `sort/2`, `memberchk` | 114 -> 134 NS, 120 -> 140 2P |
| row term | `call(Ref, Arguments)` ground | evaluator | `sort/2`, `msort/2`, `keysort/2` | 810 NS / 910 2P closure rows |

Structural equality, hash, and sort are repeated at each check and each round:
`term_hash/2` 10,553 calls NS / 42,646 2P (P4), `sort/2` 6,437 calls NS (R39),
`evaluation_lower_index/7` 1,103 NS / 5,480 2P (P4). No global interning table
exists; equal ground rows are re-hashed and re-indexed per `evaluate/4` scope.

## 5. Proposed compiler_arena schema

Transient internal arena. Phase-boundary output terms are unchanged: each phase
still materializes `basement_program/2`, `checked_datalog/4`, and
`generated_program/4` exactly as today. The arena is an in-scope side structure
owned by the compile trace boundary and torn down there.

```prolog
% Canonical arrays, each sorted by its key, plus position indexes.
compiler_arena(
    units,          % list(dl7_unit)
    nodes,          % array by NodeId:      node(NodeId, Payload)
    sources,        % array by NodeId:      source(NodeId, ...8...)
    syntax_rows,    % array by kind/seq
    edges,          % array by (Owner,Name): ':'(Owner, Name, Target, Index)
    relations,      % array by Ref:          relation(Ref, Arity, KeySets)
    rules,          % array by RuleSeq:      rule(Head, Body)
    seeds,          % array by SeedSeq:      call(Ref, Args)
    origins,        % array by Kind:         origin(Kind, NodeId)
    facts,          % array by Ref:          call(Ref, Args)   (closure)
    diagnostics     % array by (Phase,NodeId,Seq)
).

% Position indexes over the arrays.
arena_node_index(NodeId, Position).
arena_relation_index(Ref, Position).
arena_edge_index(Owner, Name, Position).
arena_origin_index(Kind, Position).
```

Predicate signatures and supported modes:

```prolog
arena_new(+Units, -Arena)                                  is det.
arena_node(+Arena, +NodeId, -Payload)                      is semidet.
arena_source(+Arena, +NodeId, -SourceRow)                  is semidet.
arena_relation(+Arena, +Ref, -Arity, -KeySets)             is semidet.
arena_edge(+Arena, +Owner, +Name, -Target, -Index)         is semidet.
arena_origin(+Arena, +Kind, -NodeId)                       is semidet.
arena_rule(+Arena, ?RuleSeq, -Rule)                        is nondet.
arena_seed(+Arena, ?SeedSeq, -Call)                        is nondet.
arena_fact(+Arena, ?Ref, -Call)                            is nondet.
arena_fact_at(+Arena, +Ref, +Position, -Call)              is semidet.
```

Index build replaces the repeated `memberchk`/`sort` scans with one dense
position lookup per entity. `memberchk(relation(Ref,...), Relations)` becomes
`arena_relation(Arena, Ref, Arity, KeySets)`; `memberchk(origin(Kind,NodeId),
Origins)` becomes `arena_origin(Arena, Kind, NodeId)`. The dynamic
`evaluation_lower_index/7` four-hash index becomes one canonical arena key plus
exact stored-term unification, preserving the collision-rejection contract.

## 6. Ranked migration candidates

Ranked by measured removable work, blast radius, and semantic risk. All are
proposals; none is authorized or implemented here.

| Rank | Candidate | Measured removable work | Blast radius | Semantics |
| ---: | --- | --- | --- | --- |
| 1 | Share immutable `evaluation_rule/3` clauses within one `evaluate/4` scope, mirroring `install_shared_lower_rows/3` | P5: 107 redundant rule asserts per main scope; 214 NS / 642 2P; redundant lower/seed already 0 | `v7/src/1_libtime/0_evaluator.pl` only | Unchanged clause set; only assertion count changes |
| 2 | Precompute per-argument variable identity sets once per `resolve_rules/8`, thread into mode and safety checks | R39 `argument_variables/2` 4,648 calls / 118,406 incl; nested `sub_term` + `sort` | checker internals | Pure; identity sets are a function of the argument term |
| 3 | Replace `index_argument_hashes/5` four-position hashing with one canonical nested-vector key in the arena | R39 `index_argument_hashes/5` 3,782 calls / 81,173 incl; P4 `term_hash` 10,553 NS / 42,646 2P | evaluator lower store | Exact stored-term unification retained; collision behavior preserved |
| 4 | Arena-backed `relation`/`origin`/`edge` indexes to remove linear `memberchk` scans | P4 `memberchk/2` 94,438 NS / 198,296 2P, 13.5M / 19.0M scanned items | checker plus lowerer | Lookup result identical; ordering must stay sorted |

First candidate is small enough for one reviewed commit: it adds a shared rule
store keyed by the evaluate-scope id, changes `install_rules/3` to insert only
new rules, and changes `proves/2` to read the shared store. It mirrors the
accepted R24 pattern for lower rows. Note the saving is in `assertz`/clause-space
events, not in charged Prolog inferences, so it does not move the inference
budget; it is ranked first on safety and review size, not on inference delta.

## 7. Phase data flow and ownership

```text
 .dl7 text
    |
    v
 [reader]  dl7_unit(Origin,Digest,Forms,SourceRows,ExpansionRows)
    |        node/2 + source/8 + syntax rows      owner: dl7_text_unit/6
    v
 [expander] expand_dl7/6 -> expansion/4 rows      owner: mint_tree/10
    |
    v
 [lowerer]  module_basement(Owner, basement_program(root_graph/2, datalog_program/3))
    |        pending_edge/4 + origin/2 + rule/2 + relation/3   owner: lowerer
    v
 [checker]  check_datalog/4 -> checked_datalog(root_graph/2, datalog_program/3,
    |                              Depends(dependency/5), Strata(stratum/2))
    |        + kernel_graph rows + canonical ':'/4 edges
    v
 [comptime] evaluate/4  dynamic: evaluation_rule/3, evaluation_seed/3,
    |        evaluation_lower_index/7, evaluation_request/2
    |        table: proves/2     owner: evaluate/4 scope (setup_call_cleanup)
    v
 [freeze]   generated_program(GeneratedRelations, GeneratedRules, _, _)
    |        -> next round; final re-lower/re-check
    v
 [emit]     runtime checked_datalog/4 -> compiler rows + runtime program

Ownership rule: lower_store_scope/1 owns shared lower rows for one evaluate/4
call; EvaluationId owns per-stratum rules, seeds, requests, and proves/2 table;
the compile trace frame owns the stratum memo. No arena exists today.
```

## Probe receipts and reproduction

```bash
cd v7
timeout 20 swipl -q -s /private/tmp/dl7_probe_reader.pl   -g main -t halt -- test/fixtures/lexical_binding/7_nearest_shadow.dl7
timeout 20 swipl -q -s /private/tmp/dl7_probe_debug.pl    -g main -t halt -- test/fixtures/lexical_binding/7_nearest_shadow.dl7
timeout 20 swipl -q -s /private/tmp/dl7_probe_entities.pl -g main -t halt -- test/fixtures/2_partial.dl7
timeout 20 swipl -q -s /private/tmp/dl7_probe_wrap.pl     -g main -t halt -- test/fixtures/2_partial.dl7
timeout 20 swipl -q -s /private/tmp/dl7_probe_scope.pl    -g main -t halt -- test/fixtures/2_partial.dl7
timeout 20 swipl -q -s bench/1_compiler_profile.pl -g profile_main -t halt -- test/fixtures/2_partial.dl7 /private/tmp/prof_2_partial
```

P4 and P5 run both fixtures; commands above show one. The two
`prolog_profile` call-count-only probes (`/private/tmp/dl7_probe_calls.txt`,
`/private/tmp/partial_calls.txt`) took 6.29 s and 14.74 s and provided only the
`redo` column for `member/2`; no inference claim uses them.

## Measured versus proposed

Measured: P1-P6 counts, cardinalities, scanned totals, duplicate install counts,
and the R39/R40/R41 predicate costs. Proposed: the `compiler_arena` schema,
predicate signatures, and the four migration candidates. Not measured: the
inference delta of any candidate, the cross-round split of P6 duplicate counts,
and the assoc-internal scan lengths for `get_assoc`/`put_assoc`. No kernel, type,
binding, or phase-boundary change is proposed or required.

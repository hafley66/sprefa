# DL7 cold comptime attribution

Base SHA `bab0c80e590dd474b74a0a4a050a259ff015acda`. Diagnostic, read-only.

## Contents

1. [Primary capture](#1-primary-capture)
2. [Final comptime decomposition](#2-final-comptime-decomposition)
3. [Rules installed per stratum and relation](#3-rules-installed-per-stratum-and-relation)
4. [Distinct tabled `proves/2` variants per relation](#4-distinct-tabled-proves2-variants-per-relation)
5. [Answers produced per relation](#5-answers-produced-per-relation)
6. [Duplicate answers rejected](#6-duplicate-answers-rejected)
7. [Table completion and reuse](#7-table-completion-and-reuse)
8. [Published rows per relation](#8-published-rows-per-relation)
9. [Top predicates by self time and counts](#9-top-predicates-by-self-time-and-counts)
10. [Dominant call path into `proves/2` with modes](#10-dominant-call-path-into-proves2-with-modes)
11. [Cost separation](#11-cost-separation)
12. [Next three optimization candidates](#12-next-three-optimization-candidates)
13. [Method and scope](#13-method-and-scope)

## 1. Primary capture

One traced run of the fixture:

```text
DL7_TRACE=steps timeout 15 swipl -q -g
  "use_module('v7/src/2_comptime/2_compiler'),
   use_module('v7/src/2_comptime/1c_compiler_cacher'),
   clear_compiler_caches,
   compile_dl7('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7',Rows,Rt,Dg)"
```

Phase summary, source compile is the second triple:

```text
COMPILE-TRACE program=7_nearest_shadow
  read=3/3325 expand=82/565641 lower=2/10041 check=5/39473 comptime=7/71646
  lower=39/83994 check=135/837419 comptime=563/4770209 total=846/6446937
capture wall_ms=848 inferences=6448850 rows=810 diagnostics=[]
```

The two `lower`/`check`/`comptime` triples are the standard macrotime program
(232 closure rows, first triple) and the source program (second triple). The
final comptime fixpoint is `4,770,209` inferences / `563 ms`. The published
compiler row count is `810`.

Source-compile round steps:

| step | wall_ms | inferences | rules | seed_rows | closure_rows | derived_rows | frozen_edges |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| evaluate_round(1) | 114 | 1,035,190 | 120 | 1,334 | 1,338 | 4 | 528 |
| evaluate_round(2) | 123 | 1,036,368 | 120 | 1,336 | 1,341 | 5 | 529 |
| assemble_round(1) | 1 | 6,014 | | | 810 | | |
| assemble_round(2) | 1 | 6,021 | | | 811 | | |

Two rounds reach a stable freeze (`NextEdges == FrozenEdges` on round 2).
Round 2 adds one intern request. The instrumented comptime steps account for
`2,083,593` of `4,770,209`; the remaining `2,686,616` is
`finish_evaluation` work (`final_checked_program` re-lower/re-check, then
`continue_source_refreeze`), none of which emits a step row.

```mermaid
flowchart LR
  R["read 3.3k"] --> E["expand 566k"]
  E --> L1["lower 10k"]
  L1 --> C1["check 39k"]
  C1 --> T1["macrotime comptime 72k"]
  T1 --> L2["lower 84k"]
  L2 --> C2["check 837k"]
  C2 --> T2["source comptime 4.77M"]
  T2 --> L3["final lower/check inside comptime 2.69M"]
```

## 2. Final comptime decomposition

Exclusive self-inference attribution for the source `evaluate_checked` call,
measured with `library(prolog_wrap)` wrappers that subtract child subtrees
(`/private/tmp/probe10.pl`). Wrapper overhead inflates the window by about 8%
(total `6,875,399` vs `6,397,296` unwrapped); ratios are the signal, absolute
totals are upper bounds.

| key | predicate | calls | exclusive self inferences |
| --- | --- | ---: | ---: |
| dr | `dl7_evaluator:dependency_requirements/4` | 8,208 | 2,670,624 |
| dcf | `dl7_evaluator:demand_cone_fixpoint/6` | 54 | 430,014 |
| cdl | `dl7_checker:check_datalog/4` | 1 | 357,018 |
| rl | `dl7_evaluator:relax_levels/3` | 8,316 | 336,206 |
| ig | `dl7_evaluator:install_evaluation/5` | 14 | 252,624 |
| crr | `dl7_checker:check_resolved_rules/5` | 3 | 209,445 |
| cc | `dl7_evaluator:collect_closure/2` | 14 | 206,120 |
| gcd | `dl7_evaluator:strict_cycle_diagnostics/3` | 6 | 191,886 |
| es | `dl7_evaluator:evaluate_strata/9` | 16 | 127,043 |
| lfu | `dl7_compiler:lower_final_units/5` | 1 | 84,398 |
| vfr | `dl7_evaluator:validate_functional_rows/3` | 2 | 54,576 |
| gd | `dl7_evaluator:goal_dependencies/4` | 2,472 | 48,487 |
| rd | `dl7_evaluator:rule_dependencies/2` | 726 | 37,027 |
| ce | `dl7_evaluator:clear_evaluation/2` | 14 | 26,112 |
| srwd | `dl7_evaluator:stratify_rules_with_dependencies/4` | 6 | 10,579 |

Stratification relaxation (`dr` + `rl` + `gcd` + `gd` + `rd` + `srwd` + `rel`)
totals `3,298,620` exclusive inferences, about two thirds of the final
comptime. `dependency_requirements/4` alone is `2,670,624`, the single largest
item.

## 3. Rules installed per stratum and relation

Per-stratum installed rule counts (source round, levels 0 through 6), from the
`evaluate_install` step metrics:

`29, 23, 53, 21, 17, 75, 2` (sum 220 rule assertions per round).

Per-relation heads in the largest stratum (`stratum_rules=75`), captured by
wrapping `install_evaluation/5`:

| head relation | rules |
| --- | ---: |
| `ref(kernel(:))` | 12 |
| `ref(kernel(node))` | 11 |
| `ref(kernel(head))` | 2 |
| `ref(owner(prelude,reader_node(prelude,448)))` | 4 |
| 35 distinct `owner(prelude,_)` relations | 1 to 4 each |

The remaining strata carry no `kernel(:)`/`kernel(node)` heads; their rules are
one to four `owner(prelude,_)` definitions each. Round 2 installs the same
shape because `generated_rules=0`.

## 4. Distinct tabled `proves/2` variants per relation

Every `proves/2` entry recorded by wrapping the predicate is fully ground
(`[g,...]` patterns). Under variant tabling each distinct ground call is a
distinct subgoal table. Distinct variants per source stratum and relation:

| stratum | distinct variant calls per relation |
| --- | --- |
| round 1 level 6 (`evaluation_7`) | `:` 529, `node` 140, `product` 136, `edge_snapshot` 528, `module` 2, `nil` 1, `cons` 1, `intern` 1 |
| round 1 level 6 (`evaluation_8`) | `:` 529, `node` 140, `product` 136, `edge_snapshot` 528, `intern` 1, `module` 2, `nil` 1 |
| round 2 level 0 (`evaluation_9`) | `edge_snapshot` 529, `intern_snapshot` 1, `module` 2, `nil` 1, owners 2 |
| round 2 level 1 (`evaluation_10`) | `edge_snapshot` 529, `intern_snapshot` 1, `module` 2, `nil` 1, owners 2 |
| round 2 level 5 (`evaluation_14`) | `:` 529, `node` 140, `product` 136, `edge_snapshot` 529, `cons` 1, `intern` 1, `intern_snapshot` 1 |
| round 2 level 6 (`evaluation_15`) | `:` 529, `node` 140, `product` 136, `edge_snapshot` 529, `intern` 1, `intern_snapshot` 1 |

Full per-stratum table in `/private/tmp/probe6_out.txt`. The `edge_snapshot`
relation carries the whole 529-row frozen colon set into each stratum that
demands it; round 2 level 0 and level 1 alone request 529 snapshot subgoals
each on top of the 1,587 answer-generating entries.

## 5. Answers produced per relation

Answer-generating `proves/2` entries per source stratum, from
`/private/tmp/probe5_out.txt` (each ground entry is one answer):

| stratum | dominant relations, answers |
| --- | --- |
| round 1 level 6 | `:` 529, `node` 140, `product` 136, `edge_snapshot` 528, `nil` 20 |
| round 2 level 0 | `edge_snapshot` 1587, `nil` 12, `intern_snapshot` 2 |
| round 2 level 1 | `edge_snapshot` 1587, `nil` 6, `intern_snapshot` 1 |
| round 2 level 5 | `:` 529, `product` 136, `edge_snapshot` 529, `nil` 14 |
| round 2 level 6 | `:` 529, `node` 140, `product` 136, `edge_snapshot` 529, `intern` 1 |

The `edge_snapshot` count of 1,587 in round 2 strata 0 and 1 is the frozen
colon set plus the intern snapshot re-proved through the table in each
demanding stratum.

## 6. Duplicate answers rejected

SWI's answer insert counts, process-global (`$tbl_wkl_add_answer/4`):

```text
calls 13,909   exits 13,876   fails 33
```

33 attempted answer insertions were rejected as duplicates across the whole
process. This is a process-global table statistic, not a per-relation count;
SWI exposes no per-relation duplicate counter.

## 7. Table completion and reuse

Per-stratum `table_statistics/2` snapshots at `collect_closure/2`, and the SWI
profiler counters:

| statistic | value | scope |
| --- | ---: | --- |
| completed tables (`$tbl_table_complete_all/3`) | 1,002 | process-global |
| destroyed tables (`$tbl_destroy_table/1`) | 1,002 | process-global |
| sum of per-stratum live table counts | 1,002 | matches the above |
| `complete_call` at each collect (round 2 level 5 max) | 137 | cumulative over live tables |
| `answers` at each collect (round 2 level 5 max) | 1,347 | cumulative over live tables |

`1,002` tables are created and torn down for a `1,341`-row final closure and
`810` published rows. Each stratum uses `setup_call_cleanup` and abandons its
tables, so the `edge_snapshot` and `:` answers are re-materialized per
demanding stratum rather than reused across rounds.

## 8. Published rows per relation

The `810` published `CompilerFacts` rows and their highest-cardinality relations
with samples (full grouping in `/private/tmp/probe6_out.txt`):

| relation | count | sample row |
| --- | ---: | --- |
| `ref(kernel(:))` | 529 | `call(ref(kernel(:)),[ref(kernel(:)),const(index),ref(primitive(int)),const(3)])` |
| `ref(kernel(node))` | 140 | `call(ref(kernel(node)),[ref(kernel(:))])` |
| `ref(kernel(product))` | 136 | `call(ref(kernel(product)),[ref(kernel(:))])` |
| `ref(kernel(module))` | 2 | `call(ref(kernel(module)),[ref(module(prelude))])` |
| `ref(kernel(nil))` | 1 | `call(ref(kernel(nil)),[const([])])` |
| `ref(owner(prelude,reader_node(prelude,16)))` | 1 | `call(ref(owner(prelude,reader_node(prelude,16))),[ref(primitive(text)),ref(application(owner(prelude,reader_node(prelude,16)),[primitive(text)]))])` |
| `ref(owner(prelude,reader_node(prelude,51)))` | 1 | `call(ref(owner(prelude,reader_node(prelude,51))),[const([])])` |

Sum `810`; all rows are `call/2`. `kernel(:)` is 65.3% of published rows. The
six source lines produce almost none of the closure; the closure is the prelude
type graph carried as colon edges and nodes.

Suspicious structure: the same 529 colon edges appear three times over the
compile, as `kernel(:)` seeds, as `kernel(edge_snapshot)` copies, and as
re-derived colon answers. That copy is the "copied graph structure" the table
counts reflect.

## 9. Top predicates by self time and counts

Self time and call/redo from `library(prolog_profile)`, whole compile
(`/private/tmp/prof_all.txt`; total time 0.772 s):

| # | predicate | calls | exits | fails | self time | children |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | `is/2` | 159,783 | 159,783 | 0 | 0.27s (35.6%) | 0.00s |
| 2 | `$memberchk/3` | 172,636 | 85,555 | 87,081 | 0.12s (15.1%) | 0.00s |
| 3 | `dl7_checker:head_variables/2` | 1,296 | 1,298 | 7 | 0.05s (6.4%) | 0.00s |
| 4 | `dl7_evaluator:demand_cone_fixpoint/6` | 15 | 15 | 0 | 0.02s (2.9%) | 0.03s (4.1%) |
| 5 | `$tbl_destroy_table/1` | 1,002 | 1,002 | 0 | 0.02s (2.1%) | 0.00s |
| 6 | `$tbl_wkl_add_answer/4` | 13,909 | 13,876 | 33 | 0.01s (1.4%) | 0.00s |
| 7 | `$garbage_collect/1` | 8 | 8 | 0 | 0.01s (0.8%) | 0.00s |
| 8 | `sort/2` | 6,407 | 6,407 | 0 | 0.01s (0.8%) | 0.00s |
| 9 | `dl7_checker:argument_variables/2` | 4,648 | 4,649 | 6 | 0.01s (0.7%) | 0.00s |
| 10 | `dl7_evaluator:dependency_requirements/4` | 9,636 | 9,636 | 0 | 0.01s (0.7%) | 0.27s (35.0%) |
| 11 | `dl7_evaluator:evaluation_seed/3` | 1,002 | 3,959 | 992 | 0.01s (0.7%) | 0.00s |
| 12 | `error:has_type/2` | 1,844 | 1,844 | 0 | 0.01s (0.7%) | 0.00s |
| 13 | `lists:member_/3` | 24,291 | 94,104 | 9,937 | 0.01s (0.7%) | 0.00s |
| 14 | `erase/1` | 12,194 | 12,194 | 0 | 0.01s (0.7%) | 0.00s |
| 15 | `ugraphs:warshall/4` | 592 | 593 | 0 | 0.01s (0.7%) | 0.00s |

SWI exposes call, redo, fail and self time per predicate, not per-predicate
inference counts; the exclusive inference column is from the wrapper probe
(section 2). `dependency_requirements/4` redo count is `0` because its inner
`findall/3` consumes all solutions; the `0.27s` children time and the `is/2` +
`$memberchk/3` self times are that body. `87,081` of `172,636` `$memberchk/3`
calls fail, the linear `memberchk` scan over the level list.

## 10. Dominant call path into `proves/2` with modes

The largest cost is not inside `proves/2`; it precedes the evaluator, in
stratification. Path and modes:

```text
evaluate_checked/4
  evaluate_compiler_rounds/11                     round 1, round 2
    evaluate/4                                    +Rules +Seeds -Closure
      rule_dependencies/2                         +Rules -Dependencies
      stratify_rules_with_dependencies/4          +Rules +Deps -Strata
        strict_cycle_diagnostics/3                ugraphs warshall, 6 calls
        relax_to_fixpoint/3                       +Deps +Levels0 -Levels
          relax_levels/4                          +Levels0 +Levels0 +Deps -Levels
            dependency_requirements/4             +Relation +Deps +Levels -Requirements
              findall/3
                member(dependency(Relation,Body,_,Gap,_), Dependencies)   scan 292
                memberchk(level(Body,BodyLevel), Levels)                  scan 76
                is/2  Required is BodyLevel + Gap                         2,670,624 self
```

`v7/src/1_libtime/0_evaluator.pl:489` `relax_to_fixpoint`,
`:496` `relax_levels`, `:508` `dependency_requirements`. Modes are ground
input at every goal; only `Requirements`/`Levels` are outputs. The relation and
level lists are re-scanned on every pass. Measured: `292` dependencies and
`76` levels per `dependency_requirements` call, `131` `relax_to_fixpoint`
invocations across the compile, `8,208` final-comptime calls.

The evaluator path for the dominant relation (`:` 529 answers) and its seed /
snapshot form:

```text
evaluate_strata/9                                 +Strata ... +Level
  evaluate_stratum_after_aggregates/13
    install_evaluation/5                          +Rules +Seeds +LowerRows
    collect_closure/2                             findall(Call, proves(Id,Call), Calls)
      proves/2                                    tabled, +Id ?Call, variant
        clause 2  evaluation_seed/3               ground seed row
        clause 3  evaluation_lower/3              evaluation_lower_index/7
        clause 4  evaluation_rule/3, instantiate_rule/3, proves_body/2
                    satisfy_goal/2 -> proves/2     recursive, table hit
    clear_evaluation/2                            abolish_table_subgoals
```

The 529 `:` and 528/529 `edge_snapshot` answers are ground seeds and frozen
lower rows, not rule derivations; the source fixture's six lines add the Holder
colon edges only. Each stratum installs its own `EvaluationId`, so the same
ground call is a fresh table per stratum and per round.

## 11. Cost separation

| category | exclusive inferences | evidence |
| --- | ---: | --- |
| fixed setup (`rule_dependencies`, aggregate derivation, install) | ~300k | `rd` 37k, `diz` 12k, `ig` 253k install writes |
| repeated traversal (stratification relaxation) | 3,298,620 | `dr` + `rl` + `gcd` + `gd` + `rd` + `srwd` + `rel` |
| answer materialization (`collect_closure`, install, clear, table inserts) | ~490k plus table engine | `cc` 206k, `ce` 26k, `ig` 253k, `$tbl_wkl_add_answer` 13,909 |
| checker validation (`check_datalog`, `check_resolved_rules`, functional keys) | ~621k | `cdl` 357k, `crr` 209k, `vfr` 55k |
| final source lowering | 84,398 | `lfu` |
| demand-cone fixpoint search | 430,014 | `dcf` |

The single repeated work is the `dependency_requirements/4` full re-scan on
every relaxation pass, more than four times the entire checker validation.

## 12. Next three optimization candidates

1. Index dependencies by head relation and relax levels incrementally.
   - Repeated work removed: the 292-element `member/2` rescan and 76-element
     `memberchk/2` per relation per pass, 8,208 calls and `2,670,624` self
     inferences; replace with a per-head body list and a worklist that only
     revisits relations whose inputs changed.
   - Semantic surface: stratification levels only (`relax_to_fixpoint` /
     `relax_levels`); the `Dependencies` list contract is unchanged.
   - Counterexample that must remain correct: `a :- b. b :- c. c.` (positive
     chain, all level 0) and `d :- not e. e.` (`e` level 0, `d` level 1), plus
     the nearest-shadow cross-stratum `(: Name (Option text))` demand path.
2. Memoize `stratify_rules_with_dependencies/4` on the exact `Rules` list
   within one compile.
   - Repeated work removed: `check_datalog` (`1_checker.pl:37`),
     `check_resolved_rules/5` (`1_checker.pl:61`) and `evaluate/4` each
     re-stratify the same 120 rules; `generated_rules=0` makes rounds 1 and 2
     and both checks identical, so the whole `3,298,620`-inference pass repeats.
   - Semantic surface: keying and invalidation only; no change to the strata.
   - Counterexample that must remain correct: a fixture with
     `generated_relations`/`generated_rules` non-zero (for example
     `2_partial.dl7`) must not reuse a stratum set computed for different
     `Rules`.
3. Stop re-seeding the full frozen edge set as `edge_snapshot` subgoals per
   stratum.
   - Repeated work removed: round 2 levels 0 and 1 each re-prove 1,587 ground
     `edge_snapshot` entries through a fresh table, and 1,002 tables (with
     `$tbl_destroy_table` and `$tbl_wkl_add_answer` costs) exist for an
     810-row output.
   - Semantic surface: round isolation and negation freshness
     (`evaluate_compiler_rounds` snapshot semantics); must preserve the rule
     that each round starts from authored seeds and the previous round's frozen
     edges.
   - Counterexample that must remain correct: a rule whose negation or
     aggregate reads a frozen colon edge must see exactly the prior round's
     edge set, no stale conclusion, and `frozen_edges` must still differ
     between rounds when edges are added.

## 13. Method and scope

- Primary capture ran the fixture once with `DL7_TRACE=steps` under
  `timeout 15`.
- Attribution used `library(prolog_wrap)` wrappers around named compiler and
  evaluator predicates, subtracting child subtree inference deltas for
  exclusive self counts. Probes are under `/private/tmp` (`probe10.pl`,
  `probe6.pl`, `probe8.pl`, `probe9.pl`).
- `library(prolog_profile)` supplied per-predicate call/redo/self time over the
  whole compile; it does not report per-predicate inferences or per-relation
  table statistics, so per-relation answers come from `current_table/2`-style
  enumeration replaced by wrapping `proves/2` and `table_statistics/2`.
- Global table counters (`$tbl_*`, duplicate rejects) are process-global and
  named as such. No per-relation duplicate or completion counter is exposed by
  SWI 10.0.2.
- Wrapper inference totals run about 8% above the untraced trace; category
  ratios use exclusive self deltas, which cancel most of that inflation.
- No source file was modified.

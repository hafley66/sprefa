# 52. DL7 post-reservation compiler profile
Date 2026-09-11. Read-only worktree at `3f29608aed20c8e4c9d17f5c37cd92325c5a16c5`
(receipt 50). SWI-Prolog 10.0.2 arm64-darwin. No source, test, workflow, skill,
V6, or Rust file edited; this receipt is the only write. Probes under
`/private/tmp` and `/tmp`. One SWI process at a time; every probe under 3 s.
## Procedure
Fixture `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Each
measurement is one fresh SWI process, tracing off unless named, with
`clear_compiler_caches/0` before the timed compile and an external `timeout`.
`P` = `/private/tmp/dl7_post_reservation_profile.pl` run as
`swipl -q -s P -g main -t halt -- <mode>`; `S` = `/tmp/scan_one.pl` with a
`rn|po|re|cs|erc|rs` selector. The pinned gate is
`v7/bench/0_compiler_performance.pl`.
## Canonical output and cold baseline
Canonical bytes: `write_canonical(output(Rows,Runtime,Diagnostics))`, SHA-256.
| Quantity | Value |
| --- | --- |
| Compiler rows / diagnostics | 810 / 0 |
| Runtime relations / rules / seeds | 132 / 120 / 0 |
| Warm inferences | 2,240 |
| Canonical SHA-256 | `fcbe44b3c70643581ab1183e3e64f1f19be64e301ad0eeee55739eca80695670` |
| Cold inferences `P baseline` x5 | 1,927,128 (identical each run) |
| Cold wall `P baseline` x5 | 309, 315, 317, 322, 329 ms (median 317) |
| Pinned gate cold | 283 ms, 1,927,130 inferences, exit 0 |
| Pinned gate warm | 3 ms, 2,240 inferences |
Harness and gate differ by 2 charged inferences on identical source, inside
finalizer variance; rows, diagnostics, runtime, and warm inferences match
receipt 50. The hash is worktree-specific: the canonical term embeds the
absolute fixture path (see representative terms), so it is not comparable to
receipt 50's `ef2cf7a0…`, captured under a different path.
## Stage timing
`P steps`, one process, `DL7_TRACE=steps`, execution order. Traced counter is
step-instrumented; use ratios, not the total. Whole traced compile = 332.05 ms
/ 2,082,082 inferences.
| Phase occurrence | Wall ms | Inferences |
| --- | ---: | ---: |
| read | 2.26 | 3,334 |
| expand | 60.72 | 550,333 |
| lower | 1.42 | 10,838 |
| check | 2.08 | 16,187 |
| comptime | 6.98 | 66,978 |
| lower | 17.84 | 91,962 |
| check | 40.15 | 130,536 |
| comptime | 189.91 | 1,143,615 |
Tail comptime interval is 57 percent of traced wall; `evaluate_round(2)` alone
is 60 ms / 314,005 inferences.
## Table statistics
`evaluate_collect` metrics; `global_table_*` accumulate per `evaluate/4` scope
and the matching cleanup releases them.
| Collect step | Answers | Complete calls | Space (B) |
| --- | ---: | ---: | ---: |
| main r2 stratum 0 | 1,067 | 1,134 | 992,296 |
| main r2 stratum 5 | 676 | 73 | 598,672 |
| main r1 stratum 1 | 530 | 565 | 427,560 |
| cleanup r2 stratum 0 | -1,067 | -1,134 | -992,296 |
Every cleanup reports `leftover_lower_rows=0`; post-compile residue is
`arena_reservation` clauses = 0 and `reservation_arena_scope` rows = 0,
reproducing receipt 50's zero-residue claim.
## Predicate call counts, redos, failures
`P calls` (`library(prolog_profile)`), one process; self wall run-specific.
| Predicate | Calls + redos | Exits + fails | Self wall |
| --- | ---: | ---: | ---: |
| `$memberchk/3` | 65,155 + 0 | 24,730 + 40,425 | 21.3% |
| `is/2` | 70,968 + 0 | 70,968 + 0 | 16.6% |
| `dl7_checker:head_variables/2` | 648 + 7 | 649 + 6 | 6.0% |
| `assertz/1` | 7,476 + 0 | 7,476 + 0 | 5.1% |
| `lists:append/3` | 49,638 + 0 | 48,457 + 1,181 | 4.7% |
| `dl7_checker:argument_variables/2` | 2,697 + 7 | 2,698 + 6 | children 4.3% |
| `occurs:sub_term/2` | 2,697 + 15,665 | 15,665 + 2,697 | 0.8% |
| `dl7_evaluator:arena_stratum/3` | 18,921 + 0 | 11,373 + 7,548 | id indexed |
| `dl7_lowerer:arena_reservation/6` | 9,466 + 0 | 1,164 + 8,302 | JITI indexed |
| `dl7_lowerer:scoped_reservation/5` | 2,070 + 0 | 602 + 1,468 | arena dispatch |
`$memberchk/3` is boot-Prolog memberchk (`init.pl:76`), not a C builtin, so its
65,155 calls and 40,425 failing full scans are all charged. `occurs:sub_term/2`
carries 15,665 redos for 2,697 calls: each `argument_variables/2` call walks
every nested subterm and leaves choicepoints.
C-level versus charged inferences: `sort/2` (3,838 calls), `msort/2`,
`keysort/2`, `term_hash/2`, `length/2`, `is/2`, and subterm traversal are C
builtins; SWI charges no inference per comparison or list cell, so zero readings
understate CPU. Resolver scans are charged Prolog recursion inside `memberchk`.
## Parent and child inclusive cost
`profile` self+children and the step tree (wall, coarse); overlapping, do not sum:
`compile_dl7/4` 98.3%, `compile_after_reads/4` 66.8%, `evaluate_checked/4`
48.9%, `check_datalog/4` 27.2%, `read_dl7/5` 18.1%, `resolve_name/6` 12.8%.
`resolve_name/6` is leaf-heavy: the `memberchk` scan lives in its own body, not
in its children (the recursive call and `resolve_target/3`).
## List-cell scan measurements
Each row wraps one user predicate with `prolog_wrap:wrap_predicate`, sums
`length/2` of its ground list arguments per call, and runs a cold compile in a
fresh process. Calls include recursive entries. Cardinalities: pending `Edges`
= 484, module `Nodes` = 234, checker `DerivedStrata` = 120.
| Predicate (call site) | Scanned list | Calls | Cells |
| --- | --- | ---: | ---: |
| `dl7_checker:resolve_name/6` (`1_checker.pl:584`) | `Edges`+`Nodes` | 2,050 | 1,346,880 |
| `dl7_checker:parent_owner/3` (`1_checker.pl:598`) | `Edges` | 1,510 | 666,472 |
| `dl7_lowerer:callable_slot/4` (`0_lowerer.pl:1263`) | `Edges` | 2,614 | 1,539,637 |
| `dl7_checker:relation_stratum/3` (`1_checker.pl:1077`) | `DerivedStrata` | 788 | 46,384 |
| `dl7_lowerer:expression_reserved_callable/4` (`0_lowerer.pl:1605`) | `Relations` | 528 | 54,448 |
| total measured | | 7,490 | 3,653,821 |
The three pending-edge scans are 3,552,989 cells, 5.1 times the 692,571
reservation cells receipt 50 removed. `scoped_reservation/5` scans 0 list cells:
`scoped_reservation_list/5` is never called on this fixture; all 2,070 lookups
dispatch to `scoped_reservation_arena/5`.
## Representative output terms
Read, not only counted. The absolute path is why the canonical hash is
worktree-specific.
```text
relation0 = relation(ref(kernel(:)), 4, [[0,1],[0,3]])
rule0 = rule(call(ref(kernel(:)), [ref(module(file('<repo>/v7/.../7_nearest_shadow.dl7'))),
        const('Name'), var(derived_bind(reader_node('<repo>/...',0))), const(0)]),
        [checked_goal(positive, call(ref(owner(prelude,reader_node(prelude,16))),
        [ref(primitive(text)), var(derived_bind(reader_node(...,0))) ]))])
row0 = call(ref(kernel(:)), [ref(kernel(:)), const(index), ref(primitive(int)), const(3)])
seed0 = none          diagnostics = []
```
## Comparison with receipt 50
Only the reservation-lookup procedure is identical. Receipt 50's wrapper counted
2,070 `scoped_reservation/5` lookups; the profiler counts 2,070 calls, 1,468
failing, 0 list cells. Receipt 50's after cold inferences were 1,927,130; the
pinned gate reproduces 1,927,130, a 2-inference delta from `P`. Receipt 50's
after wall samples 295/294/283/293 ms bracket this gate's 283 ms. No other
receipt-50 number is reused.
## Nominated next target
**One pending-edge lookup arena serving `resolve_name/6`, `parent_owner/3`, and
`callable_slot/4`.** Largest measured removable repeated work, one physical key
shared by both modules, reusing the accepted JITI-arena mechanism of receipts 47
and 50.
```prolog
dl7_checker:resolve_name(+Owner,+Name,+Edges,+Nodes,+Visited,-Resolved) is semidet.
dl7_checker:parent_owner(+Owner,+Edges,-Parent) is semidet.
dl7_lowerer:callable_slot(+CallableTerm,+Environment,+Index,-Slot) is semidet.
```
`Owner`, `Name`, `CallableTerm` ground on every call; `Edges`, `Nodes`,
`Visited`, `Environment` ground; outputs trail. `resolve_name/6` walks parents
with its `Visited` guard; `callable_slot/4` matches `target(Callable)` against
`pending_edge(Callable, Candidate, _, Index)`. Lifetime owner: the checker
boundary that opens the origin arena in `2_compiler.pl`
(`compile_units_traced/3`) owns the `resolve_name`/`parent_owner` view; the
lowerer environment boundary owns the `callable_slot` view; same thread-local
innermost-first scope stack and `setup_call_cleanup/3` teardown with
`retractall/1` by store id as receipt 50.
Current repeated-work equation (measured, per cold compile):
```text
cells = 2050*(len(Edges)+len(Nodes)) + 1510*len(Edges) + 2614*len(Edges)
      = 1,346,880 + 666,472 + 1,539,637 = 3,552,989 list cells
```
Proposed SWI mechanism: flattened dynamic
`arena_pending_edge(Owner,Name,Target,Index,StoreId,View,Sequence)` with a JITI
argument index on `(Owner,Name)` for the forward lookup, a deep index on the
target-owner argument for `parent_owner/3`'s reverse search, and keyed
module-node membership facts; exact stored-term unification after bucket
selection; assert order preserves first match.
Semantic gates: same-owner first match and product-before-any-kind precedence;
`Visited` cycle termination; `edge_origin` arena provenance for `resolved`; list
accessors stay available when no scope is active; generated-callable sequence
compatibility checked before the arena serves a checker call (receipt 47 guard);
`check_datalog/4` and `lower_datalog/5` phase terms byte-identical; diagnostics,
rule order, strata, closure rows unchanged.
Expected observable reduction: ~3.55M scanned list cells removed per cold
compile. Charged inferences near flat (receipt 50: -692,571 cells cost +0.92
percent inferences). Wall bounded by receipt 50's ratio (10.9 percent median for
692,571 cells) and load-sensitive, so secondary.
Missing probe: `$memberchk/3`'s remaining ~61,595 calls are unattributed by cell
count, because `memberchk/2` is a static builtin `wrap_predicate` cannot wrap.
The five wrapped predicates cover the largest scan sites only; a larger
unattributed scan would change the ranking.
## Reproduction
```bash
cd v7
timeout 60 swipl -q -s bench/0_compiler_performance.pl -g main -t halt -- \
    test/fixtures/lexical_binding/7_nearest_shadow.dl7
timeout 15 swipl -q -s /private/tmp/dl7_post_reservation_profile.pl -g main -t halt -- baseline
timeout 15 swipl -q -s /private/tmp/dl7_post_reservation_profile.pl -g main -t halt -- steps
timeout 15 swipl -q -s /private/tmp/dl7_post_reservation_profile.pl -g main -t halt -- calls
timeout 25 swipl -q -s /tmp/scan_one.pl -g main -t halt -- rn
timeout 25 swipl -q -s /tmp/scan_one.pl -g main -t halt -- cs
```
`scan_one.pl` selectors `rn|po|re|cs|erc|rs`; each is one fresh process. `P`
modes `baseline|terms|steps|tables|calls|pdata|scanc`.

# 53. JITI pending-edge and module-node graph lookup arena

Date: 2026-09-11. Implementation worktree based on `135a7a0ae` (receipt 52).
SWI-Prolog 10.0.2 arm64-darwin. One physical storage change was made: the
measured repeated pending-edge and module-node list scans in
`dl7_checker:resolve_name/6`, `dl7_checker:parent_owner/3`, and
`dl7_lowerer:callable_slot/4` now read from one shared JITI-backed graph lookup
module. The language, kernel, binding rules, checker results, compiler rows,
phase terms, and evaluator are unchanged. No install, build, push, or merge was
run.

## TOC

- [Pre-edit inventory](#pre-edit-inventory)
- [Gate 0 baseline](#gate-0-baseline)
- [Physical representation](#physical-representation)
- [Boundaries, identity, and cleanup](#boundaries-identity-and-cleanup)
- [Semantics preserved](#semantics-preserved)
- [Realized JITI shape](#realized-jiti-shape)
- [Exact output parity](#exact-output-parity)
- [Cold before and after](#cold-before-and-after)
- [Work counters and cleanup](#work-counters-and-cleanup)
- [Tests and CI coverage](#tests-and-ci-coverage)
- [Measured versus proposed](#measured-versus-proposed)
- [Reproduction and Boop-Check](#reproduction-and-boop-check)

## Pre-edit inventory

The shared storage module is new; the two consumers keep their signatures and
their list fallbacks:

```prolog
dl7_checker:resolve_name(+Owner, +Name, +Edges, +Nodes, +Visited, -Resolved)
dl7_checker:parent_owner(+Owner, +Edges, -Parent)
dl7_lowerer:callable_slot(+CallableTerm, +Environment, +Index, -Slot)
```

`resolve_name/6` scans `Edges` for `pending_edge(Owner, Name, Target, _)`, walks
the reverse parent edge, then tests `memberchk(module(Owner), Nodes)` for the
kernel and primitive fallbacks. `parent_owner/3` scans `Edges` for
`pending_edge(Parent, _, target(Owner), _)`. `callable_slot/4` scans `Edges` for
`pending_edge(Callable, Candidate, _, Index)` and keeps atom `Candidate` labels,
with the clause cut making a first non-atom candidate fall through to `none`.

Callers of the three accessors:

| Accessor | File | Callers |
| --- | --- | --- |
| `resolve_name/6` | `v7/src/2_comptime/1_checker.pl` | `resolve_target/5`, `resolve_call/5`, `resolve_argument/4`, recursive parent walk |
| `parent_owner/3` | `v7/src/2_comptime/1_checker.pl` | `resolve_name/6` only |
| `callable_slot/4` | `v7/src/2_comptime/0_lowerer.pl` | `callable_slots/4`, called from `partial_bind_rules/11`, `lower_call_mode/5`, `lower_expression_call/9`, `apply_expression_operator/8` |

The checker store is owned by one `check_datalog/4` invocation; the lowerer
store is owned by one `lower_datalog/5` invocation. `check_resolved_rules/5`
does not use these accessors and keeps its path.

## Gate 0 baseline

Canonical output is `write_canonical(output(Rows, Runtime, Diagnostics))`
hashed with SHA-256 in this worktree. Counts wrap the three accessors with
`library(prolog_wrap)` and sum `length/2` of the scanned list arguments per
call.

| Quantity | Baseline |
| --- | --- |
| Compiler rows / diagnostics | 810 / 0 |
| Runtime relations / rules / seeds | 132 / 120 / 0 |
| Canonical SHA-256 | `0f76b61ccd63e37e49b2e77ccaff2615461cb74a21ab064ae5b0b67e0549dd81` |
| Pinned gate cold wall / inferences | 334 ms / 1,927,130 |
| Probe cold wall / inferences | 310 ms / 1,927,128 |
| Warm inferences | 2,240 |
| `resolve_name/6` calls / scanned cells | 2,050 / 1,346,880 |
| `parent_owner/3` calls / scanned cells | 1,510 / 666,472 |
| `callable_slot/4` calls / scanned cells | 2,614 / 1,539,637 |
| Combined scanned cells | 3,552,989 |

The count reproduces receipt 52 exactly.

## Physical representation

One new shared module, `dl7_graph_lookup`, in
`v7/src/2_comptime/0_graph_lookup.pl`. The numeric name sorts before
`0_lowerer.pl`, the lowerer consumer, and the checker reaches it through
`0_lowerer`. Two dynamic fact families and one thread-local scope stack:

```prolog
:- dynamic arena_pending_edge/6.   % StoreId, Owner, Name, Target, Index, Sequence
:- dynamic arena_module_node/2.    % StoreId, Owner
:- thread_local graph_store_scope/1.
```

Lifecycle and query surface:

```prolog
open_checker_graph_store(+Edges, +Nodes) is det.
open_lowerer_graph_store(+Edges) is det.
close_graph_store is det.

graph_forward(?Edges, +Owner, +Name, -Target) is semidet.
graph_parent(?Edges, +Owner, -Parent) is semidet.
graph_callable_slot(?Edges, +Callable, +Index, -Label) is det.
graph_module_member(?Nodes, +Owner) is semidet.
```

`open_graph_store/2` mints a `dl7_graph_store_*` id with `gensym/2`, pushes it
with `asserta/1`, and installs the pending edges and the `module/1` terms of
`Nodes` with `assertz/1` in list order. `Sequence` is the authored list ordinal.
The four `graph_*` predicates dispatch on the innermost active scope and query
the JITI facts; with no active scope they use the original `memberchk/2` over
the passed list. JITI narrows candidates by the realized key index, and the
stored full terms are still unified, so a collision cannot change the result.

## Boundaries, identity, and cleanup

- Checker: `check_datalog_body/4` extracts `root_graph(Nodes, PendingEdges)` and
  opens one store per `check_datalog/4` call around the new
  `check_datalog_graph_body/8` tail. Compiler fixpoint rounds merge fresh
  basements, so each call's edge and node lists can differ; the store is built
  from the current lists and is never reused across checker calls.
- Lowerer: `lower_after_declarations/7` opens one store per `lower_datalog/5`
  call over `PromotedVisibleEdges`, nested inside the existing reservation
  arena `setup_call_cleanup/3`. Promotion runs before the store is opened and
  does not call `callable_slot/4`.
- Identity: every clause carries its `StoreId`; the scope stack is thread-local
  and innermost first. Nested lowerings or checker calls, and simultaneous
  threads, use disjoint ids and scopes.
- Cleanup: `open_graph_store/2` closes its own store and rethrows if
  installation fails or throws; `setup_call_cleanup/3` runs `close_graph_store`
  on success, failure, and exception. `close_graph_store/0` pops the innermost
  id and retracts exactly that id's clauses.

## Semantics preserved

- First match: `assertz/1` insertion order reproduces the first `memberchk/2`
  match for duplicate keys (forward and parent), verified by tests and by the
  `2+3` key index on the 484-clause main store.
- Callable labels: `graph_callable_slot/4` returns the first `(Callable, Index)`
  candidate only when it is an atom, otherwise `none`; a first non-atom
  candidate does not backtrack to a later matching edge. The list branch keeps
  the identical shape.
- Parent selection and cycles: the parent walk and its `Visited` guard are
  unchanged; self and two-name cycles terminate and keep the same diagnostics.
- Module fallback: `module(Owner)` membership and the four primitive names plus
  `kernel_relation/2` are unchanged.
- List fallback: a direct call with no active store still reads the list, so
  test entrypoints that call the accessors without a boundary behave as before.

## Realized JITI shape

`library(prolog_jiti):jiti_list/1` captured inside a wrapper on
`close_graph_store/0` for every nearest-shadow store, largest first. The main
checker store realizes the forward key index; the lowerer stores realize the
callable index and the reverse Target index.

| Store | Clauses | Realized index | Buckets | Speedup | Collisions |
| --- | ---: | --- | ---: | ---: | ---: |
| main checker | 484 | arguments `2+3` (Owner+Name) | 256 | 72.0 | 37 |
| main checker | 484 | argument `4` (Target) + deep `4:1` | 4 | 1.4 | 0 |
| lowerer | 895 | arguments `2+5` (Owner+Index) | 128 | 30.3 | 35 |
| lowerer | 789 | arguments `2+5` (Owner+Index) | 128 | 27.2 | 35 |
| lowerer | 366 | argument `5` (Index) | 128 | 24.0 | 3 |
| lowerer | 37 | argument `3` (Name) | 64 | 15.0 | 4 |
| module nodes | 2 | argument `2` | 2 | 1.0 | 0 |

The main checker store takes the forward `2+3` index, so forward lookups are
narrowed on `(Owner, Name)`. The lowerer stores take `2+5` or `5` for
`(Owner, Index)` callable lookups. `graph_parent/3` binds `Target` and uses the
argument 4 index. Stored full terms are unified after bucket selection.

## Exact output parity

The fixture is `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Before
and after were serialized as `write_canonical(output(Rows, Runtime,
Diagnostics))` and hashed with SHA-256 in this worktree.

| State | Rows | Diagnostics | SHA-256 |
| --- | ---: | ---: | --- |
| before | 810 | 0 | `0f76b61ccd63e37e49b2e77ccaff2615461cb74a21ab064ae5b0b67e0549dd81` |
| after | 810 | 0 | `0f76b61ccd63e37e49b2e77ccaff2615461cb74a21ab064ae5b0b67e0549dd81` |

The pinned compiler performance gate also passed cold and warm output parity,
the 810-row checkpoint, empty diagnostics, and runtime 132 relations, 120
rules, 0 seeds.

## Cold before and after

Each row is one fresh `swipl` process, tracing off, `clear_compiler_caches`
before the timed compile, run serially under `timeout 60`. The pinned gate was
measured with the changes stashed for the before set.

| State | Sample | Cold wall ms | Cold inferences |
| --- | ---: | ---: | ---: |
| before | 1 | 298 | 1,927,130 |
| before | 2 | 284 | 1,927,130 |
| before | 3 | 285 | 1,927,130 |
| before | 4 | 286 | 1,927,130 |
| before | 5 | 280 | 1,927,130 |
| after | 1 | 239 | 1,947,223 |
| after | 2 | 234 | 1,947,223 |
| after | 3 | 235 | 1,947,223 |
| after | 4 | 233 | 1,947,223 |
| after | 5 | 238 | 1,947,223 |

Median cold wall decreased from 285 ms to 235 ms, a decrease of 50 ms or 17.5
percent. The two ranges are disjoint (before 280 to 298, after 233 to 239).
Charged cold inferences changed by `+20,093`, or `+1.04` percent. Warm
inferences are unchanged at 2,240. The source is kept because the compiler-level
wall improved materially; the inference change is small and positive.

The fixpoint-heavy `2_partial.dl7` profile (`DL7_TRACE=collect`) has eight
closure rounds and therefore repeated `check_datalog/4` calls with different
edge and node lists. Before and after both compile to 910 rows, empty
diagnostics, runtime 139 relations, 127 rules, 1 seed, and 8 closure rounds.
Cold inferences changed from 4,316,621 to 4,337,426, `+0.48` percent. That
profile's pinned 15,562-row checkpoint is stale in this worktree and fails
identically before and after this change, so it is not a regression signal.

## Work counters and cleanup

Instrumented with `library(prolog_wrap)` on the lifecycle and query predicates,
one fresh nearest-shadow compile.

| Counter | Before | After | Delta |
| --- | ---: | ---: | --- |
| checker graph store builds | 0 | 4 | +4 |
| lowerer graph store builds | 0 | 6 | +6 |
| checker pending-edge assertions | 0 | 1,042 | +1,042 |
| checker module-node assertions | 0 | 514 | +514 |
| lowerer pending-edge assertions | 0 | 2,578 | +2,578 |
| store setup cost (checker / lowerer) | 0 | 0.67 ms / 1.60 ms | +2.27 ms |
| forward lookups served by JITI | 0 | 2,050 | +2,050 |
| parent lookups served by JITI | 0 | 1,510 | +1,510 |
| callable-slot target lookups served by JITI | 0 | 1,748 | +1,748 |
| module-node lookups served by JITI | 0 | 1,506 | +1,506 |
| lookups served by list scan | 7,490 | 0 | -7,490 |
| pending-edge/module-node list cells elided | 3,552,989 | 0 | -3,552,989 |

`callable_slot/4` reports 2,614 calls total; the 1,748 served by the JITI store
are the `target(Callable)` calls, and the remainder are the kernel and fallback
clauses, which never touched the edge list authoritatively. Residue after the
compile:

```text
arena_pending_edge clauses = 0
arena_module_node clauses = 0
graph_store_scope rows = 0
```

Zero residue holds after success, failure, exception, and injected
partial-install failure.

## Tests and CI coverage

New focused file `v7/test/23_graph_lookup.test.pl` reports 20 passing cases in
1.14 s, each case under 0.01 s:

| Case group | Coverage |
| --- | --- |
| forward equivalence | bound and partial keys, duplicate-key first match |
| parent equivalence | first-match, two-owner cycle termination and failure |
| callable slot | label/index equivalence, non-atom first match, missing fallback |
| node membership | module membership equivalence, primitive/kernel fallback |
| compiler fixtures | direct/chained aliases, unknown name, self cycle, two-name cycle, generated callable, compound labels, nearest-shadow 810 rows |
| isolation | nested store, simultaneous threads, success/failure/exception cleanup, partial-install cleanup |
| checker reuse | two different edge lists in sequence prove no stale reuse |

Bounded groups, one SWI process at a time, all pass:

| File | Result |
| --- | --- |
| `18_binding_symmetry.test.pl` | 16/16 |
| `19_lexical_binding.test.pl` | 19/19 |
| `20_compiler_performance.test.pl` | 17/17 |
| `21_compiler_profile.test.pl` | 25/25 (from repo root) |
| `22_determinism_evidence.test.pl` | 9/9 (from repo root) |
| `1_entrypoints.test.pl` | 3 failures before and 3 after, identical path-dependent ids |

CI coverage: repository test coverage adds one file with 20 cases; changed and
removed cases are zero. Workflow files are unchanged, so CI workflow coverage
adds, changes, and removes zero cases.

## Measured versus proposed

Measured: the baseline counters and cells, the realized JITI indexes per store,
the canonical output hash, the cold wall and inference samples, the store build
and assertion counts, setup cost, arena lookup counts, and zero residue.
Proposed: none. No kernel, type, binding, checker, evaluator, emitter, or phase
boundary change is made or required.

## Reproduction and Boop-Check

```bash
cd v7
timeout 60 swipl -q -s bench/0_compiler_performance.pl -g main -t halt -- \
    test/fixtures/lexical_binding/7_nearest_shadow.dl7
timeout 120 swipl -q -g "use_module(library(plunit)), \
    consult('test/23_graph_lookup.test.pl'), run_tests(dl7_graph_lookup), halt"
```

From the repository root for the profile and determinism groups:

```bash
timeout 180 swipl -q -g "use_module(library(plunit)), \
    consult('v7/test/21_compiler_profile.test.pl'), \
    run_tests(dl7_compiler_profile), halt"
timeout 180 swipl -q -g "use_module(library(plunit)), \
    consult('v7/test/22_determinism_evidence.test.pl'), \
    run_tests(dl7_determinism_evidence), halt"
```

Changed files:

- `v7/src/2_comptime/0_graph_lookup.pl` (new)
- `v7/src/2_comptime/0_lowerer.pl`
- `v7/src/2_comptime/1_checker.pl`
- `v7/test/23_graph_lookup.test.pl` (new)
- this receipt

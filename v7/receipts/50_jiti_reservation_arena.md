# 50. JITI reservation arena for the DL7 lowerer

Date: 2026-09-11. Implementation worktree based on
`e511b2ffcc9e1d5f2a15ae63fe9a56e966c36553`. SWI-Prolog 10.0.2 arm64-darwin.
This receipt covers only the `scoped_reservation/5` lookup in the lowerer. The
language, kernel, binding rules, checker results, compiler rows, and phase
terms are unchanged. One physical storage change was made:
`v7/src/2_comptime/0_lowerer.pl` now answers reservation lookups from a
lowering-boundary JITI arena instead of repeated list scans.

## TOC

- [Scope and method](#scope-and-method)
- [Pre-edit inventory](#pre-edit-inventory)
- [Gate 0 baseline](#gate-0-baseline)
- [Physical representation](#physical-representation)
- [Boundary, identity, and cleanup](#boundary-identity-and-cleanup)
- [Semantics preserved](#semantics-preserved)
- [Realized JITI shape](#realized-jiti-shape)
- [Exact output parity](#exact-output-parity)
- [Cold before and after](#cold-before-and-after)
- [Work counters and cleanup](#work-counters-and-cleanup)
- [Tests and CI coverage](#tests-and-ci-coverage)
- [Measured versus proposed](#measured-versus-proposed)
- [Reproduction](#reproduction)

## Scope and method

The repeated work is the `memberchk/2` scan inside `scoped_reservation/5`.
Every probe is one fresh SWI process under `timeout`, tracing off unless
stated. The baseline and the changed source are measured with the same fixture
(`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`) and the same
measurement procedure. Temporary probes live under `/private/tmp`. No install,
build, or delegation was performed.

## Pre-edit inventory

Signature and modes:

```prolog
scoped_reservation(+Owner, +Name, +Reservations, +Visited, -Reservation)
```

`Owner` and `Name` are the lookup key. `Reservations` is a ground list of
`reservation(Owner, Name, Target, Kind)`. `Visited` is the owner chain used to
terminate parent cycles. Every runtime call has `Owner` and `Name` bound; the
key is ground on all eleven call sites.

Callers by reservation list:

| Accessor path | Lines | Reservation list |
| --- | --- | --- |
| `scoped_alias_target/8`, `alias_terminal_deferred/5` | 179, 194 | `VisibleReservations` |
| `partial_bind_rules/11` | 410, 413 | environment reservations |
| `compound_edge_target/5` | 577 | environment reservations |
| `lower_expression/7` | 1435 | environment reservations |
| `expression_callable/4` | 1486 | environment reservations |

Two lists exist per `lower_datalog/5` call: `VisibleReservations` for deferred
alias promotion and `PromotedVisibleReservations` for the environment that
derives bind rules and executables. The public test
`same_owner_product_precedence_is_unchanged` calls `scoped_reservation/5`
directly with no boundary active, so the list path must stay available.

## Gate 0 baseline

Canonical output is `write_canonical(output(Rows, Runtime, Diagnostics))`
hashed with SHA-256 in this worktree.

| Quantity | Baseline |
| --- | --- |
| Compiler rows | 810 |
| Diagnostics | 0 |
| Runtime relations / rules / seeds | 132 / 120 / 0 |
| Canonical SHA-256 | `ef2cf7a01eac7429f272a4cc25a3d952f739d19fba5492158b0c34e2a9546425` |
| Cold wall (4 samples) | 333, 329, 326, 330 ms |
| Cold inferences | 1,909,640 |

Reservation lookup counters, measured by wrapping `scoped_reservation/5`:

| List length | Calls | Scanned cells |
| ---: | ---: | ---: |
| 37 | 184 | 6,808 |
| 118 | 11 | 1,298 |
| 119 | 11 | 1,309 |
| 366 | 932 | 341,112 |
| 367 | 932 | 342,044 |
| total | 2,070 | 692,571 |

Phase split: promotion uses 108/9/9/508/508 calls by length, lowering uses
76/2/2/424/424. The two long lists carry 98.6 percent of the scanned cells.

## Physical representation

One flattened dynamic predicate with a view column, and one thread-local scope
stack:

```prolog
:- dynamic arena_reservation/6.   % Owner, Name, Target, Kind, StoreId, View
:- thread_local reservation_arena_scope/1.
```

`open_reservation_arena/1` mints a `gensym/2` store id, pushes it, and asserts
every `VisibleReservations` entry in list order under view `visible`.
`install_promoted_reservation_view/1` then asserts this boundary's local
`PromotedReservations` in list order under view `promoted`, inside the same
scope. One store is built per `lower_datalog/5` boundary.
`close_reservation_arena/0` pops the innermost id and `retractall/1`s exactly
its clauses in both views.

`scoped_reservation/5` dispatches on an active scope. With one active it reads
the arena; otherwise it reads the list. The arena branch tries the promoted
view before the visible view, so a promoted `(Owner, Name)` shadows its
pre-promotion entry, and the two views together reproduce
`PromotedReservations ++ ImportedReservations` while each list is stored once.
The branch order stays product-first, then any-kind, then parent-walk. A JITI
collision cannot change the result because the stored full fields are unified
after bucket selection, and non-ground keys still unify against the clauses in
assert order.

## Boundary, identity, and cleanup

- One store is built per `lower_datalog/5` boundary. Nearest-shadow performs 6
  builds and asserts 1,044 visible clauses plus 818 promoted-view clauses.
- Promotion runs before the promoted view is installed, so its 206 lookups read
  the visible view exactly as the pre-promotion `VisibleReservations` list. The
  local promotion candidates (`Promotions`, `DerivedReservations`) stay lists
  enumerated once.
- The store id in every clause keeps nested lowerings and lowerings in other
  threads disjoint. The scope stack is thread-local and innermost-first.
- `open_reservation_arena/1` closes its own store when installation fails, and
  rethrows after cleanup. `setup_call_cleanup/3` runs `close_reservation_arena`
  on success, failure, and exception.

## Semantics preserved

- Nearest lexical owner: the parent walk uses the same `Owner` chain and
  `Visited` guard as the list path; the guard is re-checked on every recursive
  call.
- Same-owner ordering and product preference: facts are asserted in list order,
  and the product branch is tried before the any-kind branch.
- Partial unification: an unbound `Name` (or `Owner`) unifies against the
  clauses in order and commits to the first solution, matching `memberchk/2`.
- Parent fallback and generated callables: the environment list passed to
  `lower_after_declarations/8` is installed unchanged, so generated reservation
  rows are looked up exactly as authored.
- Cycles: two owners whose parent targets point at each other terminate and
  fail, as before.

## Realized JITI shape

After a nearest-shadow compile, `library(prolog_jiti):jiti_list/1` on
`dl7_lowerer:arena_reservation/6`:

| Index | Buckets | Speedup | Collisions |
| --- | ---: | ---: | ---: |
| argument `2` (Name) | 128 | 99.1 | 24 |
| deep `3/1/2:2` | 128 | 114.0 | 27 |
| deep `3` | 4 | 1.4 | 0 |
| deep `3:1` | 4 | 1.0 | 0 |
| deep `3/1:2` | 2 | 1.0 | 0 |

The realized key index narrows by `Name`; exact unification still filters
`Owner`, `Target`, `Kind`, `StoreId`, and `View`. `jiti_suggest_modes` was not
needed; all key arguments are called bound.

## Exact output parity

| State | Rows | Diagnostics | SHA-256 |
| --- | ---: | ---: | --- |
| before | 810 | 0 | `ef2cf7a01eac7429f272a4cc25a3d952f739d19fba5492158b0c34e2a9546425` |
| after | 810 | 0 | `ef2cf7a01eac7429f272a4cc25a3d952f739d19fba5492158b0c34e2a9546425` |

The compiler performance gate also passed cold/warm output parity, the rows
checkpoint (810), and both empty-diagnostics checks.

## Cold before and after

Each row is one fresh `swipl` process, tracing off, `clear_compiler_caches`
before the cold compile.

| State | Cold wall (4 samples) | Cold inferences |
| --- | --- | ---: |
| before | 333, 329, 326, 330 ms | 1,909,640 |
| after | 295, 294, 283, 293 ms | 1,927,130 |

Median wall changed from 329.5 ms to 293.5 ms, a decrease of 36 ms or 10.9
percent. Charged inferences changed by `+17,490`, or `+0.92` percent. Warm
inferences are unchanged at 2,240.

## Work counters and cleanup

| Counter | Before | After | Delta |
| --- | ---: | ---: | ---: |
| reservation lookups | 2,070 | 2,070 | 0 |
| lookups served by JITI | 0 | 2,070 | +2,070 |
| lookups served by list scan | 2,070 | 0 | -2,070 |
| reservation list cells | 692,571 | 0 | -692,571 |
| store builds | 0 | 6 | +6 |
| visible clauses asserted | 0 | 1,044 | +1,044 |
| promoted-view clauses asserted | 0 | 818 | +818 |

After a complete compile the residue was:

```text
arena_reservation clauses = 0
reservation_arena_scope rows = 0
```

The source remains because the compiler-level wall decreased 10.9 percent
while charged inferences changed by 0.92 percent. Output bytes, rows,
diagnostics, runtime counts, warm inferences, and residue are unchanged.

## Tests and CI coverage

`v7/test/19_lexical_binding.test.pl` gains nine focused cases and now reports
19 passing cases; the maximum case time was under 0.02 s:

- list/arena equivalence for nearest shadowing, direct and chained parent
  aliases, unknown owners, and missing names;
- list/arena equivalence with a partially bound key;
- same-owner product precedence under the arena;
- the promoted view shadowing its pre-promotion visible entry;
- a two-owner parent cycle that terminates and fails;
- unknown name fails;
- nested store isolation;
- simultaneous-thread store isolation;
- cleanup after success, failure, and exception, each with zero residue.

`v7/test/18_binding_symmetry.test.pl` (16 cases) and
`v7/test/20_compiler_performance.test.pl` (17 cases) pass. The focused test
file adds nine repository test cases; workflow files are unchanged, so CI
workflow coverage adds, changes, and removes zero cases.

`v7/test/1_entrypoints.test.pl` reports 23 failures both before and after the
change; the failing case ids are identical in the two runs and come from the
absolute worktree path in the rows-only hash, not from this change.

## Measured versus proposed

Measured: the baseline counters and cells, the realized JITI index, the output
hash, cold wall and inferences, the assertion count, and zero residue.
Proposed: none. No kernel, type, binding, checker, evaluator, emitter, or
phase-boundary change is made or required.

## Reproduction

```bash
cd v7
timeout 60 swipl -q -s bench/0_compiler_performance.pl -g main -t halt -- \
    test/fixtures/lexical_binding/7_nearest_shadow.dl7
timeout 120 swipl -q -g "use_module(library(plunit)), \
    consult('test/19_lexical_binding.test.pl'), \
    run_tests(dl7_lexical_binding), halt"
timeout 120 swipl -q -g "use_module(library(plunit)), \
    consult('test/18_binding_symmetry.test.pl'), \
    run_tests(dl7_binding_symmetry), halt"
```

Changed files:

- `v7/src/2_comptime/0_lowerer.pl`
- `v7/test/19_lexical_binding.test.pl`
- this receipt

# Bounded lexical binding review

Read-only, no source change. `/tmp/p1.pl` `/tmp/p2.pl` `/tmp/p4.pl` ran the real predicates; `/tmp/p3.pl` `/tmp/p4.pl` also simulate a pair-keyed `Visited`.

## Confirmed defects

| # | site | defect | probe result |
|---|---|---|---|
| 1 | `0_lowerer.pl:1292` `scoped_deferred_reservation/5` | filters on `deferred_expression`+`expression` only, walks past a nearer binding of the same Name | from owner `inner` returned `reservation(root,'Name',deferred_expression(..),expression)`; `scoped_reservation` returned the nearer `reservation(inner,'Name',target(base),reference)` |
| 2 | `0_lowerer.pl:1309` `scoped_callable_reservation/6` | filters on `product`/`derived_callable` + `relation/3`, same skip | `expression_callable` returned `ok(target(rel),2,[])` with `inner:'Name'` bound |
| 3 | `1_checker.pl:281-293` `resolve_name/6` | `Visited` holds Owner, not the binding, so an alias hop re-entering a walked owner is rejected | every chain in `p1.pl` FAILED, including the legal one-hop `o:A -> o:B -> target(t)` |

Clause order makes 1 and 2 user-visible: `lower_expression:1230` cuts before `:1245`, `expression_callable:1281` tries the callable walker first. Outer binding wins in both positions today. Defect 3 fires from `:264`, `:619`, `:652`, each passing `[]`; legal chain and cycle are indistinguishable, both `unresolved_name`.

## The case the two edits do NOT cover

`resolve_target/5` `:272-275` has three clauses: `target/1`, `const/1`, `name/2`. No `deferred_expression/1` clause. The skip at `:253` sits in `resolve_edges`, the emitter, not in lookup, so `resolve_name:283` still `memberchk`s the deferred edge and falls off `resolve_target`. `p4.pl`:

| chain | Owner-key today | pair-key |
|---|---|---|
| direct `o:Expr` (expression binding) | FAIL | FAIL |
| `o:A -> o:Expr` | FAIL | FAIL |
| `o:A -> root:B` (outer expression) | FAIL | FAIL |
| control `o:A -> o:B -> target(t)` | FAIL | `ref(t)` |

The pair key routes MORE chains into that cliff, so it is necessary and not sufficient for the contract clause "expression aliases follow chains to the same type". What the checker should do is a term/phase question, not a missing clause: a static checker cannot evaluate a comptime application or return an unbound value, so the reuse candidate is the lowerer's derived colon-lookup goal (`lower_expression:1230`, `var(derived_lookup(NodeId))` plus a `':'` goal). The measurement stands; the mechanism is the implementation lane's to propose.

## Smallest reuse path, as far as it goes

**Checker.** Key `Visited` on `Owner-Name` at guard `:282` and both pushes `:284`
`:286`. `p3.pl`: two-hop, three-hop, alias-to-outer and chain-ending-in-product
resolve; `A<->B`, `A->A`, `A->B->C->A` and unknown bare names fail into the
existing `unresolved_name`; nearest wins (`ref(base)`, not `ref(outer)`). No
second resolver, no graph. Necessary but not sufficient, per the section above.

**Lowerer.** `scoped_reservation/5` already walks kind-agnostically. Fold the two
filtered walkers into one call and classify the returned `Kind` at the call site
with `expression_reserved_callable/4` `:1342-1350`. A kind mismatch must NOT fall
through to an outer scope. Lowerer `Visited` stays Owner-keyed: each walker fixes
one Name and never switches names, so no chain re-enters an owner.

## Corrections to an earlier draft, and consequences

`derived_callable` is constructed at `2_compiler.pl:1096-1097`
`generated_callable_reservation/7`; the dead-code claim was wrong (grep scoped to
the lowerer), and that classification plus its tests stay. `scoped_reservation`'s
same-owner `product`-first branch `:1328-1331` is separate from nearest-OWNER
lookup; my probe fed a synthetic duplicate list, so it shows branch behavior, not
an authored bug. Preserved absent a counterexample and an explicit decision.

Callable position changes behavior: a nearer non-callable binding becomes
`error(not_relation(Name))` `:1350`, a nearer binding whose target lacks
`relation/3` becomes `error(undeclared_relation(Callable))` `:1348`. Programs
compiling today become diagnostics; the fixture count belongs in the plan.
`reservation_parent/3` `:1340` and `parent_owner/3` `:295` both `memberchk` the
parent edge, so an owner reachable as two targets picks one parent arbitrarily.

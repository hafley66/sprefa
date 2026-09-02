# DL7 compiler and evaluator semantics

## Contents

1. Compiler snapshot time
2. Stratified evaluation
3. Demand relations
4. Closure publication
5. Generated program refreeze
6. Invariants for evaluator changes

## 1. Compiler snapshot time

DL7 comptime has an observable snapshot clock. One compiler round evaluates a
complete checked program against frozen inputs from the previous round.

```text
round N closure
    colon rows  -> round N+1 edge_snapshot rows
    intern rows -> round N+1 intern_snapshot rows
```

Generated `:/4` edges, intern identities, generated relation declarations, and
generated rules become visible after a freeze boundary. The representative
`2_partial.dl7` fixture stabilizes after seven evaluations:

```text
round 1  +4 edges  +5 intern rows
round 2  +8 edges  +7 intern rows
round 3  +0 edges  +1 intern row   generated rule graph changes
round 4  +2 edges  +0 intern rows
round 5  +0 edges  +1 intern row
round 6  +1 edge   +0 intern rows
round 7  +0 edges  +0 intern rows  stable confirmation
```

Immediate `intern` or generated-edge visibility changes this clock. Treat that
as a language semantic change requiring an explicit decision, not as a local
performance rewrite.

## 2. Stratified evaluation

The checker derives relation strata before evaluation.

- Positive recursion remains within one stratum.
- A negative dependency raises the consumer by one stratum.
- An aggregate dependency raises the consumer by one stratum.
- Every stratum reads an immutable completed-lower snapshot.
- Negation checks the completed lower rows and never an in-progress current
  stratum.
- Aggregate rows are derived from complete lower proofs, then installed as
  seeds for their owning stratum.
- Native predicates for completed lower strata remain immutable. Their table
  answers can be retained while higher strata are installed and evaluated.
- Only relations on positive dependency cycles require SLG tabling. Acyclic
  relations execute as ordinary indexed Prolog predicates.
- Rule and seed ownership by stratum can be bucketed before evaluation. This
  changes list traversal only; each strict stratum still receives the same
  immutable completed-lower snapshot.

For `2_partial.dl7`, the final checked runtime program has this distribution:

```text
level 0  45 relations   34 rules
level 1  11 relations   13 rules
level 2  22 relations   41 rules
level 3   4 relations    5 rules
level 4   3 relations    3 rules
level 5   5 relations   30 rules
level 6   2 relations    3 rules
```

Nine of its runtime relations are on positive dependency cycles.

The least stratum assignment solves weighted dependency constraints of the
form `level(head) >= level(body) + gap`, where positive edges have gap zero
and negative or aggregate edges have gap one. Grouping constraints by head and
indexing the previous level vector preserves the relaxation fixpoint.

## 3. Demand relations

DL7's checked rule IR contains relational operations with mode-sensitive,
demand-driven behavior:

- `nil(?List)` supplies the canonical empty proper list.
- `cons(?Head, ?Tail, ?List)` deconstructs a ground list, or constructs a list
  when head and tail are ground.
- `intern(?Constructor, ?Arguments, ?Identity)` constructs a canonical semantic
  identity when constructor and arguments are ground.

Userland relations can acquire the same demand shape through ordered goals.
For example, `contains(?List, ?Value)` can enumerate values when `?List` is
already bound even though an all-variables query cannot enumerate a universe
of lists.

This makes the evaluator a hybrid:

```text
materialized relation rows
        +
bound relational calls that construct or deconstruct values
```

A conventional bottom-up delta engine cannot assume every useful body relation
is independently enumerable. A bound subquery can succeed while the same
relation called with all variables fails.

## 4. Closure publication

The native evaluator publishes:

- completed rows for the current stratum's declared relations;
- immutable lower rows;
- the canonical `nil([])` row;
- canonical `intern` requests observed while proving rules.

Transient demand subqueries are proof machinery. They are not automatically
published as closure rows. A measured example is
`contains([id,name], id)`: it can be used while proving a rule without becoming
a compiler output row.

An incremental evaluator that records every successful subquery changes the
closure. The exact oracle detected three extra `contains/2` rows in that design.

## 5. Generated program refreeze

Compiler rules can emit public `def`, `head`, `body`, `Apply`, and `:/4` rows.
The generated-program assembler converts those rows into checked rule IR.
After compiler closure stabilizes, source is lowered again with generated
callables available in its expression environment. The final program then
passes through ordinary name resolution and Datalog checking.

The refreeze is semantic work. Reusing an earlier basement is valid only for a
unit whose lowering result is proven independent of the generated expression
environment.

The split prelude currently satisfies that condition. Its deferred lowering
under an empty environment is term-identical to strict lowering, and strict
lowering remains term-identical under the final generated environment for
`2_partial.dl7`. Local declarations are prepended to imported reservations,
so local callable names resolve first. The test
`prelude_lowering_is_environment_independent` preserves the deferred-versus-
strict invariant across the complete prelude.

The compiler can also reuse the complete initial checked program when a strict
probe under authored declarations reproduces the initial module basements and
origins exactly. Generated relation declarations are appended to the checked
relation set. Generated rules still pass through the final resolved-rule
checker. A source unit with an unknown generated call fails the probe and uses
the complete refreeze path.

## 6. Invariants for evaluator changes

Keep these checks available:

```text
complete sorted closure equality
diagnostic equality
compiler row count
compiler round count
runtime relation, seed, and rule counts
cold and warm outputs identical
```

`DL7_VERIFY_EVALUATOR=1` compares the native evaluator with the retained generic
reference evaluator per stratum. Any future delta evaluator should have a
separate full-snapshot comparison mode and exercise all compiler rounds.

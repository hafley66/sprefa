# Higher-order relation and scan findings

## Context

The lab tested compile-time named rules against the current V6 parser, oracle,
compiler plan, SQL lowerer, TypeScript emitter, and SQLite checker. The runnable
prototype is
`v6/prolog/labs/higher_order_scan/0_receipts.pl`.

Existing semantics already form this lowering:

```text
named algorithm call
        |
        v
ordinary rel declarations and <- / <+ rules
        |
        v
checked fixed graph
        |
        v
SQLite tables, joins, keyed upserts, frontiers
```

The prototype has 7 checks:

```text
scan partitioned by key               PASS
ordered same-tick scan occurrences    PASS in oracle
switchMap replacement and late value  PASS
switchScan context reset              PASS in oracle
higher-order value erasure            PASS
real SQL and TypeScript emission       PASS for switchMap
current surface and host syntax        PASS
```

The emitted compiler still refuses `scan` and `switchScan` by the same name:

```text
unsupported_construct(edge_body_needs_pre(...))
```

`v6/prolog/ARCH.pl:708` already owns that implementation as
`pre_occurrence_loop`. Thirteen current fixtures carry this refusal.

## Type signature

The lab uses one canonical compile-time signature:

```text
sig(
  inputs,
  outputs,
  reads,
  writes,
  grade,
  cardinality,
  lifetime,
  effects
)
```

Examples:

```text
scan step:
  state<S> x event<A> -> state<S>
  write grade 0
  downstream observation grade +1
  at most one write per occurrence per key
  keyed-state lifetime

switch inner:
  event<A> x result<B> -> view<B>
  one live scope per switch key
  lifetime until the next outer value with that key
  async result arrival grade >= 1
```

Explicit signatures can construct this term first. Later inference must
construct the same term before specialization. Inference therefore changes
authoring work without changing the checked interpretation.

The current checked IR cannot hold the complete signature:

```text
relplan/5    schema + kind + key + column types
plan/6       concrete rules + rule order + edge rules
host_plan/6  host input/output columns + demand/response refs
missing      explicit grade, cardinality, lifetime, effects, read/write sets
```

Reads and writes are recoverable from the concrete graph. Grade is implicit in
arrows and frontier placement. Cardinality, lifetime, and effect obligations
have no canonical IR field today.

## Instance timeline and lifetime

### `scan`

```text
T queue:
  +event(k, a)
      read evolving state(k)
      replace state(k)
  +event(k, b)
      read the replacement from the prior occurrence
      replace state(k)

boundary:
  publish only -start/+finish
```

The oracle proves two same-tick events fold in list order. The emitted runtime
cannot execute this shape until `pre_occurrence_loop` lands.

### `switchMap`

```text
+outer(owner, target_a)
  -> keyed replace scope(owner, target_a)
  -> +demanded(target_a, owner)

+outer(owner, target_b)
  -> -scope(owner, target_a)
  -> -demanded(target_a, owner)
  -> +scope(owner, target_b)
  -> +demanded(target_b, owner)

+result(target_a, late)
  -> stored result may remain
  -> no view row because target_a has no current demand
```

### `switchScan`

```text
+context(owner, page_a) -> state(owner, page_a, seed)
+event(owner, x)        -> step page_a state
+context(owner, page_b) -> replace with page_b seed
+event(owner, y)        -> step page_b state
```

The prototype runs context reset and later events through the same ordered
occurrence queue. The keyed state row retires the old machine.

## Storage and SQL

The specialized `switchMap` graph emits fixed tables:

```text
open_scope(owner PRIMARY KEY, target) WITHOUT ROWID
demanded(target, owner)
route_result(target, value)
route_view(target, value)
```

Write sequence:

```text
1. project outer occurrence
2. INSERT open_scope
3. ON CONFLICT(owner) UPDATE target
4. recompute/increment demanded support
5. join demanded to result
6. publish boundary deltas
```

There are no function columns, runtime closures, dynamic tables, or serialized
relation values.

`sh` is the only current input/output declaration:

```dl
sh fetch(ep: text) -> (body: text) = `...`.
answer(Ep, Body) <- wanted(Ep), ? fetch(Ep, Body).
```

It expands to fixed `__host_demand_fetch` and `__host_response_fetch`
relations. `bind` only declares world-fed columns. `rel A(...) -> (...)`
does not parse. A relation name written in an ordinary value position parses as
an atom, without `RelRef` semantics.

## Decisions

1. Higher-order values are compile-time only.
2. Named state and output relations remove the need for anonymous returned
   relations.
3. `scan` lowers to keyed replacement plus ordered `pre` reads.
4. `switchMap` lowers completely into keyed scope, demand, and view rules.
   Its runtime classification is expansion-only.
5. A built-in `switchMap` remains checker-visible sugar so the scope coverage
   checker can validate parent-key flow and report the source call. The
   existing zombie-scope negative fixture proves this checker obligation.
6. `switchScan` uses the same expansion plus the ordered scan loop.
7. The only required runtime/compiler kernel work is the already-planned
   ordered occurrence loop for `pre`.

Rejected in this arc: runtime function values, executable SQLite columns,
dynamic relation creation, a switch-specific runtime, and a second state store.

## One unresolved surface choice

<!-- todo(decision): Select one compile-time named-rule spelling after the ordered occurrence loop lands; the lab proves all three erase to the same canonical signature and ordinary relation graph. -->

Exactly three ways to pay:

### 1. Symbol in a built-in algorithm slot

```text
scan(events, state, step)
```

- Syntax: parser marks `step` as a rule name only in that slot.
- Checker: resolve its canonical signature, then specialize.
- Runtime: no change beyond ordered `pre`.
- Storage: no change.
- Migration: none; current named relations and rules remain valid.

### 2. Separate directional rule declaration

```text
rule step(state, event) -> (next) ...
```

- Syntax: new declaration and printer form.
- Checker: explicit input/output mapping constructs the canonical signature.
- Runtime: same specialization and erasure.
- Storage: no change.
- Migration: new definitions only; existing rules can be wrapped or inferred.

### 3. Directional `rel` overload

```text
rel step(state, event) -> (next) ...
```

- Syntax: extends `rel` with an input/output branch.
- Checker: must distinguish stored relation declarations from rule signatures.
- Runtime: same specialization and erasure.
- Storage: directional rel signatures create no table unless a concrete
  specialized output relation is named.
- Migration: parser, printer, declaration checks, and `sh` arrow diagnostics
  must distinguish three arrow contexts.

The runnable prototype uses an AST call and does not select a surface spelling.

## V5-usurping next task

Implement `pre_occurrence_loop` in the emitted runtime:

```text
ordered frontier occurrence
  -> resolve all matching edge arms
  -> apply keyed writes
  -> expose those writes to the next occurrence's pre read
  -> continue
  -> net boundary deltas once
```

This moves thirteen current fixtures from named refusal toward compiled parity
and supplies the state transition engine required by `scan` and `switchScan`.
`switchMap` itself can proceed as checker-visible expansion sugar after that
without another runtime operation.

## Verification

Commands and measured results:

```sh
swipl -q -l v6/prolog/labs/higher_order_scan/0_receipts.pl -g go -g halt
# 7 PASS

swipl -q -l v6/prolog/conformance/go.pl \
  -g "forall(member(N,[batched_increments_both_count,counter_fold_matches_hand_computation,switch_as_keyed_replace,merge_policy,exhaust_policy,concat_program_queue,async_state_machine_with_pattern_scan,same_tick_error_then_fresh_chains_arms,ghcacher_host_program_term]),(fixture(N,_,_,_,E),engine:fixture_expectations_hold(N,E)))" \
  -g halt
# 9 selected existing-world fixtures PASS

swipl -q -l v6/prolog/compile/test/run_sql_check.pl \
  -g "check(switch_as_keyed_replace)" -g halt
# exit 0

swipl -q -l v6/prolog/compile/test/run_sql_check.pl \
  -g "check(demand_laziness_effect_rows)" -g halt
# exit 0
```

The lab emits a temporary TypeScript module and checks that `open_scope` SQL is
present while `scan(` and `switchMap(` are absent.

## Staffing

Sol performed the read-only production audit and isolated prototype on
`codex/rel-ref-file-span-lab`. Production parser, compiler, emitter, runtime,
and conformance fixtures were not edited. The coordinator owns integration,
ARCH updates, index regeneration, commits, and any surface ruling.

# Higher-order relations and scan

## Context

V6 already has the lower semantics needed for reactive flattening:

```text
rule + keyed Set + ordered occurrences + finalize + external rel
```

`plans/2026-07-27-switch-flow.md` proved `switchMap` as keyed replacement
over an ordinary program relation. `v6/prolog/src/kernel.pl` records
`register` as the state primitive and `external_rel` as the effect boundary.
The missing layer is compile-time composition: a named rule or relation must
be usable as an argument to `scan` or a flattening algorithm, then disappear
into an ordinary checked relation graph before SQL lowering.

The existing host spelling is directional:

```dl
sh fetch(ep: text, prev: text) -> (status: int, tag: text, body: text) =
  `...`.
```

That arrow separates host inputs from host outputs. It is not a general
function-rel declaration and does not make `fetch` a runtime function value.
`bind name(columns).` feeds world rows without an output arrow.

## Decisions

1. Higher order is compile-time only in this arc. Named relations and named
   rules may be composition arguments; no executable value is stored in a
   relation column or SQLite.
2. Every call site specializes to concrete relations and rules before type,
   clock, stratification, and SQL analysis.
3. Stateful composition uses a named keyed state relation. Anonymous returned
   relations are unnecessary for the first implementation.
4. `scan` is the state algorithm:

```text
scan : Seed<S> x Event<A> x Step<S,A,S> -> Event<S>
```

5. `switchMap`, `mergeMap`, `exhaustMap`, `concatMap`, and `switchScan` must
   be expressible from `scan`, key shape, guards, retraction, `finalize`, and
   the external relation boundary. A construct that cannot lower through that
   chain needs a failing real program before any kernel addition.
6. Unification remains the rule-body model. Directional flow applies to
   effectful or state-transition rule arguments where input and output
   positions affect clock, lifetime, and execution.
7. The first implementation may require explicit signatures. Later inference
   must elaborate to the identical canonical signature and must not change how
   a declared type or clock is interpreted:

```text
explicit source ─┐
                 ├─> canonical signature ─> checked graph
inferred source ─┘

canonical signature =
  columns + keys + input/output mode + ring + grade
  + cardinality + lifetime + read/write/effect sets
```

Rejected for this arc: runtime closures, function-valued rows, opaque host
function IDs, dynamic relation creation, and a second switch runtime.

## Target lowering

```text
point-free composition
  -> specialize named relation/rule arguments
  -> generate named state/scope/queue relations
  -> ordinary <- and <+ rules
  -> ring, clock, cardinality, lifetime, and SCC checks
  -> SQLite joins, keyed writes, frontiers, and typed host plans
```

`switchScan` is the strongest required receipt:

```text
outer context changes
  -> finalize old keyed machine
  -> initialize new keyed state
  -> route occurrences to its specialized step rule
  -> scan in occurrence order
  -> publish replacement deltas
```

The runtime execution question is concrete:

```text
arrival at T
  -> ordered occurrence queue
  -> read keyed state
  -> run specialized Step
  -> replace state
  -> publish -old/+new
  -> continue queue T or promote delayed consequences to T+1
```

<!-- todo(decision): Determine whether existing rel declarations plus a separate named rule declaration can express Rule<A,B>, or whether the existing rel A(inputs) -> (outputs) note should become the directional rule signature. -->

<!-- todo(feature): Prototype scan, switchMap, and switchScan as expansion-only algorithms over the existing kernel and record the first case that cannot lower. -->

<!-- todo(bug): Reconcile scan's required ordered occurrence loop with the thirteen current pre fixtures before claiming same-instant reducer semantics. -->

## Questions for the lab

1. What exact syntax already parses for relation input/output signatures,
   host signatures, rule parameters, and named relation references?
2. Can a named rule argument be resolved syntactically and specialized without
   adding a general `RelRef` runtime value?
3. What minimal static signature is needed: input/output columns alone, or also
   reads, writes, grade, cardinality, and lifetime?
4. Can `scan` use the planned ordered occurrence loop directly?
5. Can `switchScan` reinitialize state and retire the old support cone without
   an extra tick phase?
6. Which examples expose a real difference between unification flow and
   directional effect/state flow?
7. Can the existing shell `->` spelling share a signature model without making
   shell declarations the semantic precedent for pure rules?
8. Where should the canonical signature live so explicit declarations and
   future inference produce byte-identical checked IR?

The lab has a budget of three unresolved choice points. It must select the
scrappiest runnable shape compatible with the current parser and engine. For
each choice point it may:

1. settle it with an existing-world receipt;
2. pay it with one explicit restriction or implementation cost; or
3. return exactly three priced alternatives showing syntax, checker, runtime,
   storage, and migration consequences.

It must not stop at an ambiguity list.

## Verification

The lab must use the existing parser, expansion pipeline, oracle, compiler,
emitter, and SQLite runtime. Paper-only examples do not settle a question.

Required receipts:

```text
scan sum/reducer                 ordered same-tick occurrences
scan partitioned by key          independent state
switchMap                        old inner retracts, late result rejected
mergeMap                         concurrent keyed inners survive
exhaustMap                       active guard rejects new outer occurrence
concatMap                        finalization promotes the next queued outer
switchScan                       context replacement resets reducer state
specialization                   emitted graph contains no function value
clock proof                      inferred grade matches observed tick log
signature equivalence            explicit and inferred fixtures lower identically
SQL proof                        fixed named tables and indexed key access
host proof                       async completion re-enters as a later arrival
```

Run the narrow lab first. Promote fixtures only after the lowering hypothesis
survives. The full sweep budget is one final run.

The handoff must identify the next V5-usurping task unlocked by the result and
show whether a built-in `switchMap` is expansion-only, checker-visible sugar,
or requires one new kernel operation.

## Staffing

Sol performs a read-only design and prototype audit with medium direction,
starting from commit `ad66a83ba9a62a6557665ce8b392aa835e5bfdf1`.
It may create lab files and this plan may be amended from measured results.
The coordinator owns integration, ARCH rows, generated indexes, commits, and
any surface decision.

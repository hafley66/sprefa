# Reference membership boundary

## Context

Commit `b90bb264` replaced stored struct dictionaries with public target
relation tables. A world value such as a finding containing a span currently
causes the resolver to insert the span row, look up its dense `__id`, and write
that endpoint into the finding row.

The corpus measures the incomplete boundary:

- tick logs: 101 identical, 0 wrong, 2 recorded run errors;
- final state: 92 identical, 9 target-visibility differences, 2 recorded run
  errors;
- SQLite final state contains resolver-created `span`, `place`, and
  `repo_body` rows;
- the resolver inserts those rows before `runTick`, so their creation produces
  no target-relation delta.

A public relation row that is queryable in final state but absent from its
relation clock gives current membership and delta membership different
histories.

Commit `87610329` also pins endpoint stability: keyed target replacement uses
`ON CONFLICT DO UPDATE`, preserving `__id`. `INSERT OR REPLACE` can strand
stored parent endpoints by deleting and recreating the target row.

## Existing contracts

```text
resolveReferences(
  plans: RelationReferencePlan[],
  arrivals: ArrivalBatch
) -> Observable<ArrivalBatchWithIntegerEndpoints>

tick(
  arrivals: ArrivalBatchWithIntegerEndpoints
) -> Observable<TickDeltas>
```

Current instance timeline:

1. decode complete target rows carried in parent wire values;
2. conflict-check target keys;
3. insert missing target rows directly;
4. look up target `__id`;
5. rewrite parent reference fields;
6. start the ordinary tick.

Current storage:

```text
target(__id, key..., fields...) UNIQUE(key...)
parent(..., target_id INTEGER, ...)
```

There are no stored semantic JSON, rendered JSON, path copies, or nullable
reference payloads.

## Decision card

<!-- todo(decision): Select how resolver-created public target membership participates in the relation clock. -->

### Option 1: normalize nested wire values into same-tick target arrivals

Proposed timeline:

```text
normalize(arrivals)
  -> target assertion arrivals
  -> apply target assertions
  -> resolve target ids
  -> apply rewritten parent arrivals
  -> one combined tick delta
```

Ramifications:

- target membership and target deltas agree;
- nested host/world payloads retain current ergonomics;
- target and parent can become visible in one logical tick;
- runtime needs an ordered two-phase arrival application;
- oracle absorbs the same normalization;
- the 9 final-state differences become expected target rows and gain tick-log
  receipts.

### Option 2: require target arrivals before reference arrivals

Proposed timeline:

```text
tick N: +target(...)
tick N or N+1: +parent(target-key-shaped wire value)
```

Ramifications:

- ordinary arrival machinery owns every public membership change;
- resolver becomes lookup-only and missing targets refuse or suppress parents;
- providers and fixture schedules must emit target and parent rows separately;
- same-tick support still needs ordering when both occur in one batch;
- wire volume grows by one target assertion per new entity, with batching and
  key deduplication available before SQLite.

### Option 3: keep resolver materialization delta-silent

Ramifications:

- no runtime phase change;
- current tick logs remain compatible;
- public snapshot queries expose rows with no corresponding arrival history;
- host demand, `latest`, and departure semantics cannot derive a uniform clock
  for those rows;
- the 9 final-state differences require a permanent oracle exception.

### Option 4: split identity catalog from public membership

Storage:

```text
target_identity(__id, key..., fields...)
target_membership(__id)
```

Ramifications:

- silent identity materialization and public membership are distinguishable;
- adds a second table and join for every referenced relation;
- reintroduces the storage-plane duplication removed by relation unification;
- retention and replacement need cross-table invariants;
- final-state visibility becomes explicit.

## Decisions

- Keep keyed target `__id` stable across non-key replacement.
- Keep full target rows in one typed relation table with integer parent
  endpoints.
- Keep `ref` unregistered. Existing target scans capture identity, typed
  variables transport it, and `decode` reads fields.
- Keep recursive inline reference declarations refused. Cyclic graphs use an
  ordinary edge relation with two entity endpoints.
- Options 3 and 4 remain documented because they match current code and the
  prior dictionary architecture respectively. Neither has been selected.

## Verification

The selected implementation must add real oracle/emitter fixtures for:

1. one nested target and parent appearing at their exact ticks;
2. two parents sharing one target assertion;
3. same-key/equal-row deduplication;
4. same-key/different-row named conflict with zero parent writes;
5. keyed non-key replacement preserving parent endpoint identity;
6. target retraction while a parent remains, checked by boundary antijoin;
7. host output containing a referenced row;
8. statement count flat in batch row count.

Required gates:

```text
Prolog construction lab
Prolog ref necessity lab
Prolog plunit
conformance
tsv2 typecheck
tsv2 reference runtime tests
sweep in incremental and naive modes
ARCH.pl go
```

## Staffing

- Implementer: one Codex agent.
- Worktree: current `codex/rel-ref-file-span-lab` branch.
- Base: `87610329`.
- Suite budget: focused receipts on each edit, full sweep before commit.
- No parser, keyword, operator, or host declaration spelling changes in this
  arc.

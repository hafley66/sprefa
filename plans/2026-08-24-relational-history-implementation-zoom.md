# Relational history implementation zoom

Generic generated-relation construction is complete. The remaining compiler
work starts with history-specific relation properties. Runtime capture remains
unimplemented.

## Z1: Whole shape

```ts
// Imaginary TypeScript rendering of the intended DL6 semantics.

type KeyOf<S extends Relation> =
  Pick<S, MembersMarkedWithKey<S>>;

type History<S extends Relation> = {
  ...KeyOf<S>;

  version: int;             // increments independently for each source identity
  tick: int;                // global deterministic commit order
  recorded_at: Timestamp;   // host time sampled once per commit
  operation: "put" | "delete";

  ...NonKeyMembers<S>;      // typed source snapshot
};

type SourceIdentity<S> = Values<KeyOf<S>>;
type HistoryIdentity<S> = [SourceIdentity<S>, version];

temporal.state(User);
temporal.history(User, snapshot); // materialize History<User>
```

```ts
Source boundary delta
  -> consolidate same-commit writes
  -> allocate per-identity versions
  -> stamp commit tick and recorded_at
  -> atomically write source plus history
  -> publish committed deltas
  -> event/history reducers
```

## Z2: Each remaining part

### 1. `temporal.history(Source, Capture)` relation constructor

```dl6
enum history_capture {
  snapshot,
  delta,
  snapshot_and_delta
}.

enum retention_policy {
  all
}.

rel temporal.event(Source: type).
rel temporal.state(Source: type).
rel temporal.history(Source: type, Capture: history_capture).
rel temporal.retention(Source: type, Policy: retention_policy).
rel temporal.caused_by(State: type, Event: type).

rel History(Source: type) -> type.

rel User(
  id: key(int),
  name: text,
  email: option(text)
).

temporal.state(User).
temporal.history(User, snapshot).

# The annotation requests the canonical History(User) application.
History(Source, History(Source)) <- temporal.history(Source, _).
```

```ts
// Compiler reads canonical $type rows for User.

type HistoryUser = {
  id: int;                  // copied key
  version: int;
  tick: int;
  recorded_at: Timestamp;
  operation: HistoryOperation;
  name: text;               // copied payload
  email: Option<string>;    // copied payload
};
```

History shape policy belongs in DL6 compiler rules over the frozen `$type`
relations:

```dl6
derived_relation_request(HistoryType, History, [Source], MemberCount) <-
  type_requested(HistoryType, History, [Source]),
  history_member_count(Source, MemberCount).

derived_member_request(HistoryType, HistoryPosition, Name, MemberType) <-
  type_requested(HistoryType, History, [Source]),
  history_source_member(
    Source,
    HistoryPosition,
    Name,
    MemberType
  ).

derived_member_request(HistoryType, HistoryPosition, Name, MemberType) <-
  type_requested(HistoryType, History, [Source]),
  history_system_member(
    Source,
    HistoryPosition,
    Name,
    MemberType
  ).

derived_member_role_request(HistoryType, Position, Role, '') <-
  type_requested(HistoryType, History, [Source]),
  history_member_role(Source, Position, Role).

derived_relation_property_request(HistoryType, entity_key, KeyPositions) <-
  type_requested(HistoryType, History, [Source]),
  history_entity_key(Source, KeyPositions).

derived_relation_property_request(
  HistoryType,
  occurrence_key,
  OccurrencePositions
) <-
  type_requested(HistoryType, History, [Source]),
  history_occurrence_key(Source, OccurrencePositions).

derived_relation_property_request(HistoryType, write_policy, append) <-
  type_requested(HistoryType, History, [_]).

derived_relation_property_request(HistoryType, retention, all) <-
  type_requested(HistoryType, History, [_]).
```

Already implemented:

```dl6
# Interpreted compiler relations available to authored DL6 rules.
type_requested(Application, Constructor, Arguments).
type_field(Member, Owner, Position, Name, MemberType).
type_field_count(Owner, Count).

# Complete generated relation request rows consumed by bounded refreeze.
derived_relation_request(Application, Constructor, Arguments, MemberCount).
derived_member_request(Owner, Position, Name, MemberType).
derived_member_role_request(Owner, Position, Role, RoleValue).
```

Still required:

```dl6
derived_relation_property_request(Owner, Property, Value).

history_source_member(Source, Position, Name, MemberType).
history_system_member(Source, Position, Name, MemberType).
history_member_role(Source, Position, Role).
history_entity_key(Source, Positions).
history_occurrence_key(Source, Positions).
history_member_count(Source, Count).
history_source_key_missing(Source).
```

Prolog remains responsible for evaluating the compiler fixpoint, validating a
complete request graph, refreezing canonical rows, and emitting target
artifacts. History policy and schema derivation remain queryable DL6 data and
rules.

### 2. Per-identity `version`

```ts
type AccountIdentity = readonly [
  tenantId: int,
  accountId: int,
];

interface VersionCursor {
  historyType: TypeId;
  sourceIdentity: AccountIdentity;
  lastCommittedVersion: int;
}
```

```ts
A insert       -> A version 1
A replace      -> A version 2
B insert       -> B version 1
A replace      -> A version 3
```

```ts
function allocateNextVersion(
  tx: Transaction,
  historyType: TypeId,
  sourceIdentity: SourceIdentity,
): int {
  const cursor = tx.lockVersionCursor(historyType, sourceIdentity);
  const next = cursor.lastCommittedVersion + 1;

  tx.stageCursorUpdate({
    ...cursor,
    lastCommittedVersion: next,
  });

  return next;
}
```

Each complete source key owns an independent counter. Composite keys use the
complete ordered tuple.

### 3. `tick` and `recorded_at`

```ts
interface CommitContext {
  tick: int;
  recordedAt: Timestamp;
}
```

```ts
function beginCommit(): CommitContext {
  return {
    tick: durableEngineTick.next(),
    recordedAt: hostClock.sampleOnce(),
  };
}
```

```ts
// Both transitions happened in one commit.

[
  {
    identity: A,
    version: 3,
    tick: 42,
    recordedAt: T42,
  },
  {
    identity: B,
    version: 1,
    tick: 42,
    recordedAt: T42,
  },
];
```

```ts
version      // order within one source identity
tick         // deterministic order between commit batches
recordedAt   // observational wall time
occurredAt   // optional authored event time copied from source data
```

Pending decision:

```ts
type Timestamp =
  | UnixMilliseconds
  | UnixMicroseconds
  | RFC3339Text;
```

### 4. Put and delete history rows

```ts
type HistoryOperation =
  | { tag: "put" }
  | { tag: "delete" };
```

```ts
function transitionSnapshot<S>(
  before: S | undefined,
  after: S | undefined,
): HistoryTransition<S> | undefined {
  if (after !== undefined) {
    return {
      operation: { tag: "put" },
      snapshot: after,
    };
  }

  if (before !== undefined) {
    return {
      operation: { tag: "delete" },
      snapshot: before,
    };
  }

  return undefined;
}
```

```ts
// Existing row A is replaced twice during one commit.

beforeCommit = { id: A, balance: 10 };

intermediateWrites = [
  { id: A, balance: 20 },
  { id: A, balance: 30 },
];

afterConsolidation = { id: A, balance: 30 };

historyRow = {
  id: A,
  version: 2,
  operation: "put",
  balance: 30,
};
```

A delete stores the departing row, allowing reconstruction without consulting
another table.

### 5. Replay identity and deduplication

```ts
// Exact representation remains undecided.
type CommitId = DurableIngestionBatchId;

interface HistoryReceipt {
  historyType: TypeId;
  sourceIdentity: SourceIdentity;
  commitId: CommitId;
  allocatedVersion: int;
}
```

```ts
function captureOnce(
  tx: Transaction,
  commitId: CommitId,
  transition: ConsolidatedTransition,
): void {
  const existing = tx.findHistoryReceipt(
    transition.historyType,
    transition.sourceIdentity,
    commitId,
  );

  if (existing !== undefined) {
    return;
  }

  const version = allocateNextVersion(
    tx,
    transition.historyType,
    transition.sourceIdentity,
  );

  tx.insertHistoryRow({ ...transition, version });
  tx.insertHistoryReceipt({
    historyType: transition.historyType,
    sourceIdentity: transition.sourceIdentity,
    commitId,
    allocatedVersion: version,
  });
}
```

Without this receipt, replaying commit 42 could incorrectly allocate version 4
after version 3 was already committed.

Pending decision:

```ts
type CommitId =
  | EngineTransactionId
  | SourceBatchId
  | DurableInputOffsetRange
  | AnotherIdempotencyWitness;
```

### 6. Atomic commit-boundary capture

```ts
async function commitBatch(batch: InputBatch): Promise<void> {
  const consolidated = consolidateSourceTransitions(batch);
  const context = beginCommit();

  const published = database.transaction(tx => {
    const historyRows = [];

    for (const transition of consolidated) {
      const version = allocateNextVersion(
        tx,
        transition.historyType,
        transition.sourceIdentity,
      );

      tx.applySourceTransition(transition);

      const historyRow = {
        ...transition.snapshot,
        version,
        tick: context.tick,
        recorded_at: context.recordedAt,
        operation: transition.operation,
      };

      tx.insertHistoryRow(historyRow);
      tx.insertReplayReceipt(batch.commitId, transition, version);
      historyRows.push(historyRow);
    }

    return {
      sourceDelta: consolidated,
      historyDelta: historyRows,
    };
  });

  publishAfterCommit(published.sourceDelta);
  publishAfterCommit(published.historyDelta);
}
```

Required invariant:

```ts
transactionCommitted =>
  sourceStateChanged &&
  historyRowInserted &&
  cursorAdvanced &&
  replayReceiptInserted;

transactionRolledBack =>
  sourceStateUnchanged &&
  historyRowAbsent &&
  cursorUnchanged &&
  replayReceiptAbsent;
```

### 7. Event tables and reducers

```dl6
// Authored occurrences, including repeated equal values.
rel Payment(
  account_id: int,
  amount: int
).

temporal.event(Payment).
temporal.retention(Payment, all).

// Current keyed state plus its committed transition history.
temporal.state(Account).
temporal.history(Account, snapshot_and_delta).
temporal.retention(History(Account), all).
temporal.caused_by(Account, Payment).
```

Every temporal declaration is an ordinary compiler-relation application. The
surface uses calls, colons, commas, periods, and arrows. Relation declaration
modifiers and space-delimited policy clauses are outside the target grammar.

```ts
declare function reduceEvents<E, S>(
  events: EventLog<E>,
  initial: S,
  reducer: (state: S, event: E) => S,
): S;

declare function reduceHistory<R, S>(
  transitions: History<R>,
  initial: S,
  reducer: (state: S, transition: HistoryRow<R>) => S,
): S;
```

```ts
reduceEvents(Payment)
  // consumes every authored payment occurrence

reduceHistory(History<Account>)
  // consumes consolidated puts and deletes in version order
```

Remaining reducer work includes declaration syntax, restart checkpoints,
snapshot policy, and compaction.

### 8. Generated column namespaces and collision avoidance

Generated semantic columns use reserved dunder namespaces. The existing engine
already uses hidden physical columns such as `__id` and `__refcount`.

```ts
interface HistoryAccount {
  // Copied authored identity.
  account_id: int;

  // Compiler-owned history metadata.
  __history_version: int;
  __history_tick: int;
  __history_recorded_at: Timestamp;
  __history_operation: "put" | "delete";

  // Copied authored state.
  balance: int;
  status: string;
}
```

Event envelopes use a separate generated namespace:

```ts
interface EventPayment {
  __event_occurrence: EventOccurrenceId;
  __event_tick: int;
  __event_recorded_at: Timestamp;

  payment_id: int;
  account_id: int;
  amount: int;
  occurred_at: Timestamp;
}
```

Generated names derive from semantic member roles expressed as DL6 compiler
facts and rules:

```dl6
rel generated_member_name(role: text, name: text).

generated_member_name('history_version', '__history_version').
generated_member_name('commit_tick', '__history_tick').
generated_member_name('commit_recorded_at', '__history_recorded_at').
generated_member_name('history_operation', '__history_operation').
generated_member_name('event_occurrence', '__event_occurrence').

derived_member_role_request(HistoryType, Position, Role, '') <-
  history_generated_member(HistoryType, Position, Role),
  generated_member_name(Role, _).
```

The semantic role participates in compiler metadata. The rendered name remains
an artifact spelling and can change without changing canonical type identity.

Collision validation reserves the generated prefixes:

```dl6
rel reserved_generated_prefix(prefix: text).
rel generated_member_collision(owner: type, role: text, name: text).

reserved_generated_prefix('__history_').
reserved_generated_prefix('__event_').

generated_member_collision(Owner, Role, Name) <-
  temporal.history(Owner, _),
  type_field(_, Owner, _, Name, _),
  generated_member_name(Role, Name).
```

The generic request validator consumes collision findings and emits the named
compiler diagnostic. Prefix-wide reservation can use the same compiler
relation once compile-time text-prefix matching is admitted.

```dl6
rel Account(
  account_id: key(int),
  __history_version: int
).

temporal.history(Account, snapshot).
```

```text
reserved_generated_member(
  Account,
  __history_version,
  history_version
)
```

Physical and logical generated columns remain distinct:

```text
__id
  hidden physical row identity used by SQLite and relation references

__history_version
  visible generated semantic member queryable from DL6, TS, Rust, and SQL
```

Causality rows use both namespaces:

```ts
interface TransitionCause {
  __history_type: TypeId;
  __history_identity: SourceIdentity;
  __history_version: int;

  __event_type: TypeId;
  __event_occurrence: EventOccurrenceId;
}
```

### 9. Complete composite-key timeline test

```ts
type AccountIdentity = readonly [tenantId: int, accountId: int];

const A: AccountIdentity = [1, 100];
const B: AccountIdentity = [1, 200];

commit(10, [
  put(A, { balance: 10, status: "open" }),
]);

commit(11, [
  put(A, { balance: 20, status: "open" }),
  put(A, { balance: 30, status: "open" }),
]);

commit(12, [
  put(A, { balance: 40, status: "open" }),
  put(B, { balance: 50, status: "open" }),
]);

commit(13, [
  delete(A),
]);

restart();

commit(14, [
  put(A, { balance: 60, status: "restored" }),
]);

replayCommit(14);
```

```ts
expect(historyRows).toMatchInlineSnapshot(`
[
  [A, version 1, tick 10, T10, put,    balance 10, open],
  [A, version 2, tick 11, T11, put,    balance 30, open],
  [A, version 3, tick 12, T12, put,    balance 40, open],
  [B, version 1, tick 12, T12, put,    balance 50, open],
  [A, version 4, tick 13, T13, delete, balance 40, open],
  [A, version 5, tick 14, T14, put,    balance 60, restored],
]
`);
```

```ts
type RemainingImplementationOrder = [
  "relation property request IR",
  "temporal.history(Source, Capture) generated shape",
  "remove relation declaration modifiers for log and retention",
  "reserved generated member namespaces and collision validation",
  "timestamp representation decision",
  "durable replay identity decision",
  "atomic runtime capture",
  "event/history reducer contracts",
  "cross-target timeline golden",
];
```

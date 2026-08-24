# Relational history annotations and generated relation construction

## Context

DL6 can already represent the runtime pieces of an authored event log:

```dl6
rel event(entity_id: int, payload: text) log keep(all).
```

`log` gives occurrence-preserving edge writes, and `keep(all)` retains every
occurrence. `keep(count(N))` lowers to a real storage retraction after the tick.
The current surface and lowering are in
`v6/prolog/compile/parse_dl_dcg.pl:782-784` and
`v6/prolog/lower.pl:6610-6629`. The older language ruling describes event,
state, and history as retention capacities rather than separate channel kinds
in `v6/DECISIONS.md:98-123`.

DL6 also has the compile-time inputs needed to derive a history schema:

- `key(T)` site evidence reaches canonical member roles and current key
  normalization.
- `$type` exposes canonical declarations, members, applications, arguments,
  and roles.
- Positive compiler relations evaluate to a deterministic set fixpoint and
  erase before runtime planning.
- `type_apply/3` computes the canonical identity of a closed type application,
  requests missing specialization, and participates in bounded refreeze.

The type-application backend landed in commit `63dc2ade0` and is specified in
`issues/type-apply-refreeze/item.md`. Its authored spelling was intentionally
left open in `issues/comptime-constructor-class/item.md:39-47`.

The missing compiler operation is generated relation construction. A compiler
relation can read the key and members of `User`, and it can request an existing
constructor application such as `list(User)`. It cannot yet submit a complete
derived member set and have that set become one canonical relation type.

History also exposes a current language collision. `keyed` means replacement
identity for an edge-written Set. A Log preserves occurrences and has no key
concept. `keyed(Log)` is explicitly refused by
`v6/prolog/0_program_check.pl:125-128`. A history relation needs both:

```text
entity identity       source key members
occurrence identity   source key members + version
write policy          append one committed transition
retention             all, initially
```

Treating those four properties as one `keyed` or `log` bit would retain the
existing collision.

`seq(Partition)` supplies a visible per-partition cursor through generated
ordinary rules in `v6/prolog/0_seq_expand.pl:80-124`. It does not by itself
define the history contract. History version assignment must occur once per
committed source transition, after same-tick writes have consolidated.

`now(Tick)` reads the durable engine tick in `v6/prolog/lower.pl:1066-1077`.
Tick is deterministic commit order. A wall-clock timestamp has no current DL6
primitive and must enter once at the commit boundary. Event time supplied by a
source remains an ordinary authored field.

This plan adds no behavior by itself. V6 remains in plan and hollow-trait
review phase under `AGENTS.md`.

## Decisions

1. History is requested by an ordinary compiler relation fact whose argument
   is a relation type:

   ```dl6
   rel history(Source: type).

   history(User).
   ```

   `history(User)` is the complete first-slice authored surface. There is no
   `history` keyword, decorator grammar, `chan` declaration, or history clause
   attached to `rel`.

2. Relation properties are compiler metadata relations. The generated history
   type is marked with separate identity, write, and retention facts. Existing
   `kind/2`, `keep/2`, and `keyed/2` terms remain compatibility lowering inputs
   until their consumers read the normalized metadata relations.

3. The generated relation type reuses application identity:

   ```text
   HistoryTypeId = application(HistoryConstructorTypeId, [SourceTypeId])
   ```

   No generated-name identity and no second type registry are introduced.
   Artifact names derive from the semantic ID at the existing artifact
   boundary.

4. Generated relation construction is a general compiler facility. History is
   its first standard constructor. The facility accepts one type identity plus
   a complete set of declaration, member, role, and relation-property request
   rows, validates them, then refreezes through the existing canonical type
   pipeline.

5. A source relation must have at least one canonical key member. Composite
   keys remain ordered exactly as authored. Missing keys receive
   `history_source_key_missing(SourceTypeId)`.

6. The first history shape is flat and typed:

   ```text
   source key members, in source-key order
   version: int
   tick: int
   recorded_at: timestamp
   operation: history_operation
   source non-key members, in source-member order
   ```

   `history_operation` has `put` and `delete`. A `put` row carries the committed
   new source row. A `delete` row carries the committed departing source row.
   The full typed snapshot avoids an untyped payload blob and remains queryable
   through ordinary joins.

7. Version is one-based and monotone per complete source identity. Exactly one
   version is allocated for each committed boundary transition of that identity.
   Internal rule firings, duplicate derivations, and intermediate same-tick
   replacements do not allocate versions.

8. History order is defined by `(source key, version)`. `tick` records the
   engine commit containing the transition. `recorded_at` records host wall
   time sampled once per commit batch. Wall time does not participate in
   identity, uniqueness, conflict resolution, or replay order.

9. Event time and recorded time stay separate. A source field such as
   `occurred_at` is copied as payload. The generated `recorded_at` is system
   observation time.

10. The history relation has append-only transition semantics and full
    retention in the first slice. Its occurrence identity is
    `(source key members..., version)`. A repeated occurrence identity with an
    identical row deduplicates at the storage boundary. A repeated occurrence
    identity with different data raises
    `history_version_conflict(HistoryTypeId, Identity, Version)`.

11. History observes the consolidated boundary delta of the source relation.
    It does not lower to an edge rule that joins the source relation on every
    trigger. This prevents re-answer storms from becoming false history and
    gives replacement and deletion one defined capture point.

12. Existing authored `log keep(all)` remains the event-table mechanism. An
    event table preserves every occurrence, including equal values. A history
    relation records committed state transitions with an explicit per-entity
    version. Reducers may later consume either, according to whether the author
    wants authored events or reconstructed state transitions.

13. Per-key retention windows, reducer syntax, snapshots, compaction, as-of
    queries, and constructor-valued type parameters remain follow-on arcs. The
    generated metadata must leave room for them without assigning semantics in
    this slice.

Rejected alternatives:

- A new `history` or `chan` declaration form duplicates ordinary compiler
  relation application.
- Reusing current `keyed(Log)` preserves a contradiction already enforced by
  both compiler doors.
- Calling `seq(SourceKey)` from an ordinary edge rule assigns versions at rule
  firing time rather than consolidated commit time.
- Using `now(Tick)` as wall time collapses deterministic commit order and
  observation time.
- Storing one JSON payload removes canonical member types and field joins from
  the generated relation.
- Minting generated relation names as identity breaks the landed semantic type
  identity contract.

## Type signatures and lowering contracts

The names below describe compiler IR contracts. Authored code sees only
ordinary relation declarations and facts.

```text
history(
  +SourceTypeId
)

derived_type_request(
  +ApplicationTypeId,
  +ConstructorTypeId,
  +OrderedArgumentTypeIds,
  +OriginEvidence
)

derived_member_request(
  +OwnerTypeId,
  +Position,
  +Name,
  +ValueTypeId,
  +OrderedRoles
)

derived_relation_property(
  +OwnerTypeId,
  +Property,
  +Value
)

history_transition(
  +HistoryTypeId,
  +SourceIdentity,
  +Operation,
  +SourceRow,
  +CommitTick,
  +RecordedAt,
  -Version
)
```

Pseudo-code for compile-time construction:

```prolog
derive_history_shape(Source, History) :-
    % Require canonical source declaration and ordered key roles.
    % Compute History = application(history_constructor, [Source]).
    % Copy source key members first.
    % Add version, tick, recorded_at, and operation members.
    % Copy source non-key members.
    % Emit separate append, retention, entity-key, and occurrence-key metadata.
    % Submit one complete derived-type request for the next refreeze frontier.
    true.
```

Pseudo-code for runtime capture:

```text
capture_history(source boundary delta, commit context):
  // Consolidate source writes before observing them.
  // Group transitions by complete source identity.
  // Read and advance each identity's durable version cursor atomically.
  // Sample recorded_at once from the commit context.
  // Insert put/delete snapshots with unique identity + version.
  // Publish the history delta only after the source and history writes commit.
```

## Instance timelines and lifetimes

### Compile-time instance

```text
parse history(User)
  -> elaborate User to SourceTypeId
  -> freeze canonical $type snapshot N
  -> query source declaration, members, roles, and key order
  -> compute HistoryTypeId
  -> produce complete derived declaration/member/property request
  -> validate request as one graph
  -> refreeze canonical $type snapshot N+1
  -> lower generated history relation through ordinary target planning
  -> erase history request and compiler proof rows
```

The request and proof rows live only during compilation. The generated history
type and its runtime relation plan live in the compiled artifact.

### Runtime instance

```text
begin commit batch
  -> absorb and consolidate source changes
  -> identify each committed put/delete transition
  -> allocate next version per source identity
  -> stamp tick and recorded_at
  -> write source state and history rows atomically
  -> publish source and history deltas
end commit batch
```

The version cursor and retained history rows are durable. Tick is store-owned
and durable. Recorded wall time is evidence, not an ordering authority.

### Reload and replay

On restart, the next version is read from durable cursor state or the maximum
stored version for the identity under one specified recovery path. Replaying a
committed batch must not allocate another version. A history write therefore
needs the same durable commit identity used by source ingestion, or an
equivalent idempotency witness.

## Storage, reads, writes, and uniqueness

### Logical keys

For source:

```text
SourceIdentity = ordered values of members carrying the key role
```

For history:

```text
HistoryIdentity = SourceIdentity + Version
```

The history relation exposes both identities in compiler metadata. Entity-key
metadata drives partitioned version allocation and per-entity queries.
Occurrence-key metadata drives storage uniqueness.

### Physical storage shape

The first target shape is:

```sql
CREATE TABLE history_table (
  source_key_1 ... NOT NULL,
  ...,
  version INTEGER NOT NULL,
  tick INTEGER NOT NULL,
  recorded_at TIMESTAMP_REPRESENTATION NOT NULL,
  operation HISTORY_OPERATION_REPRESENTATION NOT NULL,
  ... copied source payload columns ...,
  UNIQUE (source_key_1, ..., version)
);
```

The generated unique constraint is insert-only history identity. It does not
select current keyed-set replacement behavior.

### Read paths

```text
complete history for entity
  WHERE source keys = ? ORDER BY version

state at version
  WHERE source keys = ? AND version <= ? ORDER BY version DESC LIMIT 1

changes after commit
  WHERE tick > ? ORDER BY tick, source keys, version
```

These are ordinary derived relations or query lowering over the generated
table. They add no retained TypeScript arrays.

### Write sequence

1. Read or lock the durable per-identity version cursor for identities changed
   in the consolidated source delta.
2. Allocate one next version per committed transition.
3. Insert history rows with the source state update in one transaction.
4. Advance cursors only if the transaction commits.
5. Publish history arrivals after commit.

Multiple identities in one commit may receive the same tick and
`recorded_at`. Their version domains are independent.

## Implementation sequence

### Phase 0: language-contract fixtures

- Add no parser feature.
- Pin `history(User)` as an ordinary typed compiler fact.
- Pin missing-key, duplicate-property, constructor-identity, and compiler-plane
  erasure diagnostics.
- Add adversarial fixtures for composite keys, optional non-key members,
  nested generic member types, module-qualified relations, and two history
  annotations targeting the same source.

### Phase 1: generated relation request IR

- Add canonical request rows for declarations, members, roles, and relation
  properties.
- Validate completeness and uniqueness before refreeze.
- Extend the existing type-application frontier to accept a complete derived
  shape for registered derived constructors.
- Reuse canonical type IDs, member IDs, freeze validation, and transport
  erasure.
- Leave runtime plans byte-identical when no generated relation request exists.

### Phase 2: history constructor

- Implement `history(Source)` as compiler rules plus the minimum interpreted
  bridge needed to emit complete derived-shape requests.
- Copy source members and roles according to the decided history shape.
- Emit entity key, occurrence key, append write policy, and keep-all retention
  as separate normalized metadata facts.
- Generate TS, Rust, JSON Schema, and catalog output from the canonical rows.

### Phase 3: commit-boundary history capture

- Add a store/runtime contract for consolidated source transitions.
- Add atomic per-identity version allocation.
- Add commit tick and one wall-time sample to the commit context.
- Write source and history rows in the same transaction.
- Emit history arrivals after commit and preserve restart idempotence.

### Phase 4: authored history slice

- Compile and execute one source relation with a composite key.
- Exercise insert, same-key replacement, second same-key replacement, another
  identity, deletion, restart, and replay.
- Verify exact history rows, versions, ticks, timestamps, operations, payloads,
  and emitted artifacts.

### Follow-on arcs

- Per-entity `keep(count(N))` and time-window retention.
- Snapshot and compaction policy for reducers.
- Reducer declarations over event or history relations.
- As-of and between-version standard relations.
- Migration of authored `log keep(...)` modifiers to ordinary relation
  annotation facts.
- Constructor-valued compiler parameters and general mapped structural types.

<!-- todo(decision): Choose the logical timestamp type and physical unit for recorded_at; preserve tick as the deterministic ordering field. -->
<!-- todo(decision): Specify the durable commit identity or idempotency witness used to prevent version duplication across restart and replay. -->
<!-- todo(feature): Add canonical generated relation request rows and validate complete declaration/member/property graphs before refreeze. -->
<!-- todo(feature): Implement the history(Source) derived constructor using canonical source key and member rows. -->
<!-- todo(feature): Add atomic commit-boundary history capture with per-identity versions, ticks, recorded time, put/delete operations, and replay safety. -->
<!-- todo(feature): Prove one composite-key history timeline through Prolog, TS plus SQLite, Rust plus SQLite, typegen, and catalog artifacts. -->

## Verification

### Compiler fixtures

- `history(User)` elaborates `User` through a `type` column and erases its
  compiler fact after expansion.
- The generated history ID equals the canonical history constructor
  application ID.
- Unrelated declaration reordering preserves source, history, and member IDs.
- Composite source key order is preserved.
- Missing source key, duplicate generated member name, duplicate position,
  malformed role, unknown source type, and conflicting repeated request have
  named diagnostics.
- Existing source `list(int)`, generic relation applications, anonymous types,
  and annotations produce unchanged canonical rows.
- A program without `history/1` produces byte-identical runtime plans.

### Runtime timeline

Use source `Account(tenant_id, account_id, balance, status)` keyed by the first
two members.

1. Insert identity A, expect version 1 `put`.
2. Replace A twice inside one commit, expect one consolidated version 2 `put`
   containing final state.
3. Replace A in another commit, expect version 3.
4. Insert identity B in that commit, expect B version 1 with the same tick and
   recorded time as A version 3.
5. Delete A, expect version 4 `delete` carrying the departed row.
6. Restart and replace A, expect version 5.
7. Replay the already committed batch, expect no additional row or cursor
   movement.

Assert the complete ordered history table as one snapshot. Assert unique
conflict behavior with an intentionally corrupted duplicate `(A, version)`.

### Existing semantics rails

- Existing `log keep(all)` continues to preserve equal occurrences.
- Existing `keep(count(N))` emits visible minus deltas.
- Existing keyed Set edge writes retain their current behavior.
- Existing `keyed(Log)` authored programs retain their named refusal until the
  metadata migration explicitly replaces it.
- Existing `now(Tick)` fixtures retain deterministic tick values.
- Existing `seq(Partition)` fixtures retain current behavior.

### CI

- Focused: parser/printer, annotation surface, compiler relations, type relation
  IR, semantic type identity, generic expansion, program checks, sequence
  expansion, retention lowering, and eventing fixtures.
- Cross-target: one generated history golden through TS plus SQLite and Rust
  plus SQLite.
- Full current Prolog compiler suite once after each integrated implementation
  phase.
- Report whether each phase adds or changes CI coverage. Formatter and linter
  output are outside CI reporting.

## Staffing

- Plan synthesis: Codex in the current workspace, no implementation worktree.
- Independent language and plan critique: Sol high in an isolated Boop
  worktree, read-only review, no implementation authorization.
- Future generated-type IR: Terra medium in an isolated worktree after user
  approval of the plan and timestamp/idempotency decisions.
- Future runtime capture: Luna medium in an isolated worktree after generated
  history artifacts exist, with Sol review before integration.
- Base SHA at plan creation: `def5dbb637a0f490cfc95b2ecadc58f64765f798`.
- Suite budget: focused Prolog suites per edit; one full Prolog run per
  integrated phase; one TS and one Rust authored runtime timeline at closeout.

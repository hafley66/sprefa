# Type values and internal deltaflow

## Implementation status (2026-07-14)

The architecture proofs and transaction prerequisites are implemented, but the
runtime tick has not been switched to delta execution yet.

Completed and covered by tests:

- content-addressed `TypeId` values, typed relation schemas, stable relation and
  rule identities, schema fingerprints, and conservative component classes;
- an exact positive-acyclic micro-engine whose touched work is independent of
  unrelated corpus size;
- process-wide SQLite cache/mmap ceilings and file-backed TEMP storage;
- a bounded staged replacement protocol with an explicit
  `StageReady { generation, key_count, row_count, digest }` seal;
- refusal of unsealed, cancelled, forged, stale, and post-seal partial stages;
- flat PLUS/MINUS plan rails, atomic row/manifest/watermark commit, failpoint
  rollback, and two-connection WAL snapshot visibility;
- `Db::insert_rows` transaction ownership and
  `Engine::with_semantic_generation`, including error and panic rollback.

The first `PreparedSourceStage` production cut is integrated for full and path
source reconciliation. Inventory/read/hash/parse work completes into a bounded,
sealed, file-backed TEMP stage before a short source transaction retracts and
inserts live facts, spans, strings, metadata, and digests. Candidate-base and
seal revalidation prevent partial or stale preparation from being treated as
an intentionally empty replacement:

```text
prepare: inventory/read/hash/parse -> bounded file-backed stage -> Ready seal
apply:   revalidate base generation -> short semantic transaction -> commit
post:    query output, perf/log output, run_gens filesystem effects
```

This first cut is connection-local and non-durable, and its producer queue is
bounded by completed-file count rather than payload bytes. Extraction families
and `RelKind` refreshers still need the same prepare/apply seam. Source apply,
family refresh, derived propagation, plan/schema fingerprints, generation
watermark advancement, effects, and scheduler acknowledgement also remain
separate transaction boundaries. Until that whole-generation boundary exists,
the current rebuild path remains the runtime fallback and the deltaflow modules
remain proofs rather than an enabled execution path.

## Context

The performance arc was initially framed around extraction ownership and SQLite
fact storage. That is supporting work, but it is not the main missing piece.
The intended change is inside sprefa itself: make the type system produce a
stable typed plan, then make sprefa's own rule engine propagate row deltas
through that plan instead of deleting and rebuilding whole downstream
relations.

The current engine is reactive only at relation granularity:

- `src/engine/strata.rs::affected_derived` computes the transitive set of
  downstream relation names from a set of changed source relations.
- `src/engine/tick.rs` scopes a tick to those names.
- `src/engine/derive.rs::rebuild_derived` then begins by executing
  `DELETE FROM rel_<name>` for every affected relation and reconstructs the
  complete relation.
- Semi-naive evaluation reduces repeated work *inside a recursive cold
  rebuild*, but it does not make an already-materialized relation accept
  deletions and insertions incrementally across ticks.

That explains the measured shape: bundling Rust type/call/dataflow extraction
reduced a one-file edit to one physical parse, yet the production tick remained
approximately seven seconds. The parser became incremental; the derived rule
network did not.

The type system is similarly split across several representations:

- `src/ast.rs::Type` is a small storage enum and `Col` carries an optional
  brand string.
- `src/typecheck.rs` rebuilds maps for brands, shapes, relations, and rule
  constraints from the whole program.
- syntax shapes are expanded away into relation columns;
- data-derived `type_decl_row` shapes are persisted at the end of one tick and
  affect declaration on the next tick;
- `derived_program_digest` hashes the entire derived layer, so any derived
  program edit is treated as a full-plan move.

Historical `feat/type-ir-value-space` is useful intent, not directly portable
code. It established the important direction that a type should be an
addressable value and structured relation data, rather than an `Arc<str>`
annotation hidden from the language. It targeted the removed v4 compile tree
and assumed callable-value machinery that is not present in v5's current
`Value { Text, Int, Null }`, so this plan ports the principle to the current
engine instead of transplanting the old modules.

The ownership plan remains useful for durable source-fact identity and bounded
staging. It must not become a second dataflow architecture. The typed plan and
delta evaluator defined here are the authority for reactivity.

## Decisions

### 1. Compile once to a stable typed dataflow plan

Introduce a storage-independent compiled representation:

```rust
struct TypedPlan {
    schema_epoch: SchemaEpoch,
    relations: IndexVec<RelId, TypedRelation>,
    rules: IndexVec<RuleId, TypedRule>,
    components: Vec<Component>,
    readers_of: IndexVec<RelId, SmallVec<[RuleId; 4]>>,
    writers_of: IndexVec<RelId, SmallVec<[RuleId; 2]>>,
}

struct TypedRule {
    id: RuleId,
    fingerprint: Fingerprint,
    head: RelId,
    inputs: SmallVec<[InputUse; 4]>,
    operator: OperatorClass,
    lowered: DeltaLowering,
}

enum OperatorClass {
    PositiveAcyclic,
    PositiveRecursive,
    Negation,
    Aggregate,
    Lattice,
    External,
}
```

`RelId` and `RuleId` are stable hashes of canonical declarations/rules, not
vector positions or source locations. A formatting edit does not move an ID.
A body change creates a new rule fingerprint while retaining the logical rule
slot needed to revalidate rows affected by replacing the old rule.

The compiler records dependency edges once. Tick-time code must not repeatedly
rediscover rule classes, strata, relation components, or column unification.

Rejected: make every operator an independent Tokio task. It multiplies queues,
retains batches per stratum, complicates SQLite ownership, and makes a simple
deterministic transaction into a distributed system inside one process.

### 2. Make types structured, interned values without changing SQLite storage

Separate a column's logical type from its physical representation:

```rust
#[derive(Clone, Copy, Eq, Hash)]
struct TypeId([u8; 16]);

enum TypeNode {
    Base(BaseType),
    Named { name: SymbolId, parent: TypeId },
    Enum { name: SymbolId, variants: SliceId<SymbolId> },
    Apply { constructor: TypeId, args: SliceId<TypeId> },
    Union { members: SliceId<TypeId> },
    Unknown { spelling: SymbolId },
}

struct TypedColumn {
    name: SymbolId,
    logical: TypeId,
    storage: StorageClass,
}

enum StorageClass { I64, InternedText, RawText, Blob }
```

`TypeId` is content-addressed from the canonical `TypeNode`; equal type values
share one arena entry. The arena is immutable for a `TypedPlan` and can be
serialized as ordinary `type_node`, `type_arg`, and `type_variant` rows for the
language to query. Generic application is structured; it is never encoded as a
string such as `Vec<HashMap<K,V>>`.

This is value-space in the current v5 architecture: a term may eventually carry
a `TypeId`, and type relations can join on it, while relation storage continues
to use compact SQLite integers/text. It does not require adding heap-heavy type
objects to every fact row.

Compatibility is staged. Existing `text`, `int`, `path`, `file`, `dir`, `repo`,
`rev`, brands, and shapes lower into `TypeNode`s first. Surface syntax and
runtime `Value` expansion happen only after parity tests prove the typed plan.

Rejected: reuse a type's display string as identity. It repeats the 0.10.0
string-interning failure mode, makes generic equality formatting-dependent, and
forces parsing on every comparison.

Rejected: immediately treat types as callable rules. That was coherent in the
old v4 value/callable model, but v5 does not currently have that value algebra.
`TypeId` leaves the callable/predicate surface possible without coupling the
incremental engine to an unported abstraction.

### 3. Distinguish schema changes from type-fact changes

A type fact is ordinary data and flows incrementally. A physical relation
schema change is a plan change and advances `SchemaEpoch`.

- changing an enum variant, generic bound, or extracted program type emits
  row deltas if no physical column layout changes;
- changing a relation's columns/storage class invalidates only that relation,
  its writer rules, and dependency-reachable consumers;
- changing a rule revalidates rows derivable through the old rule and evaluates
  the new rule;
- a global reset is reserved for incompatible metadata migrations, not every
  program digest change.

`type_decl_row` remains an epoch boundary initially: its changed rows produce a
`PlanDelta` after the current generation commits. On the next generation, only
shape references whose `TypeId` changed are recompiled. This preserves the
existing no-mid-tick-schema-mutation invariant while removing the all-program
digest blast radius.

### 4. Use signed public row transitions without a shadow provenance corpus

The internal unit of motion is:

```rust
struct GenerationDelta {
    generation: Generation,
    schema_epoch: SchemaEpoch,
    inputs: Vec<RelationDeltaRef>,
    plan: Option<PlanDelta>,
}

struct RowChange {
    fact: FactId,
    diff: i64, // public insertion (+1) or retraction (-1)
}

struct DeltaBatch {
    relation: RelId,
    generation: Generation,
    changes: Box<[RowChange]>,
    encoded_bytes: usize,
}
```

Relations retain set semantics. A downstream batch contains only actual public
presence transitions: `+1` when a row was absent and becomes present, `-1` when
it was present and becomes absent. Duplicate witnesses do not appear as
duplicate public deltas.

The first slice deliberately avoids persistent per-rule support counts. They
are correct, but they can approach another corpus-sized index and repeat the
resident/disk amplification this arc is trying to remove. Deletion instead uses
bounded candidate revalidation:

1. run the removal delta variant and materialize only candidate head rows;
2. apply the input removal to the stable relation;
3. constrain each full head rule by the candidate head keys;
4. delete a candidate only when no current rule still derives it;
5. emit `-1` only for actual public deletions.

Additions use `INSERT OR IGNORE ... RETURNING` (or an equivalent staged
anti-join on bundled SQLite) and emit `+1` only for actual inserts. This handles
projection and multiple rules: deleting one witness retains a row while another
witness exists, without storing every witness durably.

Persistent support counts remain a measured fallback if candidate revalidation
is not fast enough for a demonstrated workload. They are not an architectural
default.

Rejected: delete every negative candidate directly. It is incorrect when two
source rows project to the same output or multiple rules derive the same tuple.

### 5. Delta-lower positive acyclic rules first

For a positive rule, lowering produces delta variants rooted at the changed
input rather than a full `INSERT ... SELECT` over every stable input. Changed
relations are applied sequentially in a stable order inside one transaction.
For two changed inputs processed in `A, B` order this telescopes as
`delta(A) × B_old`, followed by `A_new × delta(B)`, so overlap is handled
exactly once without materializing every cross-product combination.
Conceptually, for `H <- A, B, C` processed in that order, the generation
observes:

```text
delta(A) × old(B)   × old(C)
new(A)   × delta(B) × old(C)
new(A)   × new(B)   × delta(C)
```

`old` and `new` are transaction states expressed through stable tables plus the
currently applied generation delta; they are not cloned relations. For each
root relation, removal candidates are captured against old state, removals are
applied and revalidated, additions are applied, then actual public transitions
propagate downstream before the next root relation is processed.

The existing `src/lower.rs::body_sql_ex` occurrence override is the initial
lowering seam: substitute a typed delta table for one positive occurrence and
pin candidate revalidation by the head key. Negated occurrences do not use this
seam and therefore remain explicit fallback.

The first production slice supports:

- positive, acyclic rules;
- equality joins, projections, comparisons, and deterministic scalar calls;
- multiple rules writing one relation;
- simultaneous inserts and deletes;
- full fallback for every unsupported component.

Negation, aggregation, lattices, recursive deletion, graph operators, and
effects remain component-scoped rebuilds until their own semantics land. A
fallback deletes/rebuilds only the unsupported component and its downstream
components, never silently pretends to be incremental.

### 6. Keep the data plane pull-based and bounded

Tokio owns the control plane, not CPU evaluation:

```text
watcher/RPC/poll
    -> coalesced GenerationIntent identities
    -> capacity-one wakeup
    -> EngineActor (sole Db owner)
    -> bounded extraction requests to Rayon(2)
    -> durable fact chunks
    -> synchronous delta propagation transaction
    -> committed generation
```

The uniform plumbing boundary is a batch source with explicit budgets:

```rust
trait BatchSource {
    async fn next(&mut self, permits: &mut BytePermits)
        -> Result<Option<BatchRef>>;
}

trait DeltaOperator {
    fn apply(
        &mut self,
        tx: &mut DeltaTx,
        input: BatchRef,
        output: &mut dyn BatchSink,
    ) -> Result<()>;
}
```

In RxJS terms the engine is:

```typescript
intent$
  .pipe(
    coalesceByRootAndPath(),
    concatMap(generation => inventoryIds(generation)),
    mergeMap(id => extractBounded(id), 2),
    bufferByBytes({ maxBytes: 256 * KiB, maxRows: 4096 }),
    concatMap(chunk => stageDurably(chunk)),
    concatMap(() => propagateDeltaTransaction()),
    concatMap(() => publishGeneration()),
  )
```

`concatMap` at generation/commit boundaries is deliberate. There is one writer
and one generation in flight. Only extraction uses concurrency two. Channels
carry identities or byte-capped chunks, never complete corpora, ASTs, or a copy
of the fact set per stratum.

Within delta propagation, operators write/read SQLite staging batches and are
called synchronously in topological order. This provides uniform backpressure
without allocating a Tokio channel for every edge.

### 7. A generation is one atomic semantic transition

The engine actor processes:

```rust
enum EngineMsg {
    Apply(GenerationIntent),
    Query(QueryRequest),
    Shutdown,
}
```

`Apply` performs:

1. coalesce source and program changes;
2. compile a `PlanDelta` from changed declarations/rules;
3. stage source `RowChange`s under byte/key/disk budgets, then seal the stage
   with its generation, complete key count, row count, and digest;
4. propagate through affected components in dependency order;
5. verify signed public transitions and candidate tables are exhausted;
6. commit rows, plan fingerprint, and generation watermark together;
7. publish the new generation and schedule effects.

Queries read the last committed generation. No query observes half-propagated
types or relations.

A clean end-of-stream is not proof that replacement output is complete. The
writer refuses every unsealed or count/digest-mismatched stage, so cancellation
or extraction failure cannot be misread as an intentionally empty replacement
that retracts all old rows. Potentially slow extraction/staging happens before
`BEGIN IMMEDIATE`; only a sealed stage may enter the short diff/apply/manifest
transaction.

This is a production prerequisite, not a later durability enhancement. The
current tick does not own one outer transaction across source refresh, derived
work, digest updates, and the generation watermark. Exact TypeFamily staging
and runtime deltaflow remain disabled until a failpoint harness proves that an
error after any one of those phases rolls all of them back together.

Old/new staging diffs use flat indexed anti-joins rooted in the bounded/staged
side:

```sql
INSERT INTO _plus_R
SELECT n.*
FROM _next_R AS n
LEFT JOIN rel_R AS o ON <full encoded key equality>
WHERE o.<first_key> IS NULL;

INSERT INTO _minus_R
SELECT o.*
FROM _changed_key_R AS k
CROSS JOIN rel_R AS o
LEFT JOIN _next_R AS n ON <full encoded key equality>
WHERE <o matches k> AND n.<first_key> IS NULL;
```

Do not lower these rails through nested queries or `EXCEPT`. Query-plan tests
must prove the changed/staged relation drives the search and persistent
relations are probed through their primary key.

### 8. Measure the engine with a synthetic micro-experiment before production

The decisive experiment does not invoke `dl` on the repository. It is a unit or
ignored integration harness built directly against `Engine` with synthetic
relations and rows.

Fixture A is a 100-relation positive chain with 10,000 rows per relation. Change
one source row. Compare:

- legacy affected-relation rebuild;
- internal deltaflow behind a feature flag.

Fixture B is a diamond where two paths derive the same output, proving
candidate revalidation retains the output after deletion of only one witness.

Fixture C changes one brand/enum/type node referenced by a small subset of
rules, proving typed-plan invalidation does not re-typecheck or reset unrelated
relations.

The experiment must report rows read, rows written, statements, maximum staged
bytes, rules re-typechecked, components visited, wall time, and RSS where the
test environment can measure it safely.

Adoption gates for the first slice:

- exact final-row parity with a clean full rebuild after every generated delta;
- one-row chain edit writes at most one visibility transition per downstream
  relation, not 10,000 rows per relation;
- no `DELETE FROM rel_*` occurs in a fully supported positive component;
- diamond deletion preserves the output until its final witness disappears;
- maximum in-memory delta payload is at most two configured chunks;
- parser/extraction counts remain invariant with 1, 10, 100, and 1,000 strata;
- unrelated type declarations cause zero re-typechecks and zero relation writes;
- candidate wall time is at most 25% of legacy on the 100 × 10,000 chain;
- candidate peak RSS is no more than legacy plus 32 MiB and has no slope with
  queued event history.

## Sequencing

### Slice 0 — measurement and typed-plan skeleton

- Add stable `RelId`, `RuleId`, `TypeId`, canonical fingerprints, and
  `TypedPlan` construction from the existing AST.
- Preserve current execution unchanged.
- Add counters for rules typechecked, components visited, relation rows deleted,
  delta rows staged, and maximum staged bytes.
- Build fixtures A/B/C and record the legacy baseline.

Gate: the typed plan is deterministic across parse/origin order, and lowering
existing programs produces the same relation metadata and diagnostics.

### Slice 1 — positive acyclic row delta

- First add the outer generation transaction and rollback failpoint harness.
- Add `GenerationDelta`, bounded staging, candidate revalidation, and delta
  lowering for one positive acyclic component.
- Feature-flag per component; all unsupported components use the legacy path.
- Run full-vs-delta property tests after randomized insert/delete sequences.

Gate: fixtures A/B pass every correctness and memory bound.

### Slice 2 — incremental plan and type changes

- Cache `TypedPlan` by item fingerprint.
- Re-typecheck only dependency-reachable rules.
- Revalidate and retract rows produced only by an edited/removed rule.
- Convert base types, brands, enums, and shapes into the `TypeId` arena.
- Emit queryable type rows without changing existing surface syntax.

Gate: fixture C plus rule-edit row revalidation passes; no global
`derived_program_digest` reset occurs for a scoped compatible edit.

### Slice 3 — negation and dirty-group aggregation

- Maintain negation witness counts per positive binding.
- Recompute only aggregate groups touched by an input delta.
- Keep explicit full-component fallback for unsupported scalar/aggregate forms.

Gate: randomized parity against legacy full rebuild.

### Slice 4 — recursion, lattices, and graph operators

- Add insertion delta to already-materialized positive recursive components.
- Choose and prove recursive deletion semantics: counting where valid, DRed
  where cyclic witnesses require re-derivation.
- Give lattices and SCC/closure operators explicit incremental contracts or
  retain measured digest/fallback boundaries.

Gate: recursion property tests include cycles, duplicate paths, deletion of one
witness, and deletion of the final witness.

### Slice 5 — surface type values

- Add `TypeId` to the language's value algebra only after the internal typed
  plan and queryable type rows are stable.
- Define type application, union, field projection, and predicate/callable
  behavior as language features rather than engine shortcuts.
- Migrate `type_decl_row` users to structured type identities with a
  compatibility decoder for base-type spellings.

Gate: type values round-trip through parse/lower/query without allocating type
trees per fact row; old brand/shape programs remain byte-for-byte compatible.

## Verification

Every implementation slice begins with a red test and runs only bounded test
binaries with `CARGO_BUILD_JOBS=2`. No production `dl`, daemon startup, corpus
scan, or repository benchmark runs without explicit approval.

Required permanent test families:

- canonical-ID golden tests for `RelId`, `RuleId`, and `TypeId`;
- typed-plan dependency and scoped invalidation tests;
- full rebuild versus delta property tests;
- duplicate-witness and signed-public-transition tests;
- rule-edit and schema-epoch rollback tests;
- crash injection before/after candidate application and before generation commit;
- EXPLAIN-plan rails rejecting persistent full scans on delta-rooted SQL;
- byte-bounded queue/staging tests;
- 1/10/100/1,000-strata memory-amplification test;
- legacy fallback parity for every unsupported operator class.

Before a production rollout, use an isolated copied database and an explicitly
approved release binary. Compare semantic digests, relation counts, per-phase
wall time, statements, rows touched, and peak RSS. The candidate is rejected if
it improves time by retaining corpus-sized deltas, ASTs, or operator queues.

## Staffing

- Base SHA: `2b10fbd6` (`main` at plan time).
- Root/expensive model owns architecture, invariants, review, and cross-slice
  integration only.
- Luna owns bounded mechanical slices after their red tests and interfaces are
  written: IDs/fingerprints, counters, fixtures, bounded staging CRUD, and
  individual delta-lowering and candidate-revalidation forms.
- Terra reviews type/dataflow semantics, transaction boundaries, recursive
  deletion, and memory accounting; it does not perform routine implementation.
- Agents work in isolated worktrees for overlapping slices. No agent may run
  `dl`, a daemon, a corpus scan, or an unbounded benchmark.
- Rayon and Cargo jobs default to two. Formatting runs once immediately before
  a requested commit, not during implementation.

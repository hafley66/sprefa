# Shared SQLite Frontier State

## Context

Both executable targets consume the same SQLite lowering. `compile_program_phases/8` builds one `Plan`, calls `lower_program/2`, computes boot statements, then passes the same `Lowered` value to either target emitter (`v6/prolog/compile.pl:701-721`).

`emit_ts:emit_program/5` and `emit_rust:emit_program/5` both receive:

```prolog
lowered(Name, Ddl, ArrivalStatements, EdgeStatements,
        LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)
```

The generated PokeAPI TypeScript program measured 6,082,867 bytes for a 42,992-byte DL6 source. Its largest sections were:

| section | bytes |
| --- | ---: |
| SQLite DDL | 1,725,668 |
| relation catalog | 1,846,669 |
| incremental relation plans | 1,373,563 |
| final-select SQL | 411,596 |

The static TSV2 runtime is 170,976 bytes. The generated artifact already imports that runtime, but the shared lowerer still specializes transient frontier, delta, support, and projection mechanics per relation.

## Type signatures

Compiler-side target shape:

```prolog
lower_program(+Plan, -Lowered).

lowered_program_data(
    +Plan,
    -program_data(Relations, Rules, Boot, Hosts, Queries)
).

relation_data(
    RelationId,
    SemanticRef,
    PhysicalTable,
    Columns,
    Key,
    Materialization
).

rule_data(
    RuleId,
    HeadRelationId,
    Inputs,
    SpecializedSql
).
```

Runtime-side target shape:

```ts
type RelationId = number;
type RowId = bigint;
type Tick = bigint;
type Sign = 1 | -1;

interface IProgramData {
  relations: readonly IRelationData[];
  rules: readonly IRuleData[];
  boot: readonly IBootData[];
}

function loadProgram(data: IProgramData): IGenProgram;
function applyTick(program: IGenProgram, arrivals: readonly IArrival[]): Promise<ITickDeltas>;
```

Rust uses the same fields through `ProgramJson` and constructs the existing engine program from them.

## Instance timeline

For an authored materialized relation:

```text
compile
  relation declaration
      -> one relation_data row
      -> one typed durable SQLite table

engine boot
  create durable typed tables
      -> create shared frontier/support tables once
      -> install compact rule metadata

tick N
  validate and intern an arrival
      -> upsert/delete the durable typed row
      -> write (relation_id, row_id, tick, sign) to shared frontier
      -> evaluate affected rules
      -> update shared support counts
      -> publish boundary deltas

drain
  advance until shared frontier has no rows for the current logical time
```

One typed materialized DL6 relation owns one durable SQLite table. Derived relations marked ephemeral own a view or rule plan. Derived relations marked materialized own one typed table under the same rule.

## Storage

Durable relation storage remains typed and relation-specific:

```sql
CREATE TABLE pokemon (
  __id INTEGER PRIMARY KEY,
  id INTEGER NOT NULL,
  name INTEGER NOT NULL,
  species INTEGER NOT NULL,
  UNIQUE (id)
);
```

References store the target row's integer `__id`. Text interning, enum identities, option identities, and list identities retain their existing scalar boundary contracts during this arc.

Transient state becomes shared:

```sql
CREATE TEMP TABLE frontier (
  relation_id INTEGER NOT NULL,
  tick INTEGER NOT NULL,
  row_id INTEGER NOT NULL,
  sign INTEGER NOT NULL CHECK (sign IN (-1, 1)),
  PRIMARY KEY (relation_id, tick, row_id, sign)
);

CREATE TEMP TABLE support_count (
  relation_id INTEGER NOT NULL,
  row_id INTEGER NOT NULL,
  rule_id INTEGER NOT NULL,
  count INTEGER NOT NULL,
  PRIMARY KEY (relation_id, row_id, rule_id)
);
```

Frontier rows refer to typed durable rows by `(relation_id, row_id)`. They do not copy a JSON or BLOB payload.

Read sequence for a rule input:

```sql
SELECT typed.*
FROM frontier f
JOIN <typed relation table> typed ON typed.__id = f.row_id
WHERE f.relation_id = ? AND f.tick = ?;
```

Write uniqueness:

- Durable identity remains the relation's declared key, or its existing all-column identity rule.
- Frontier uniqueness is relation, tick, row, and sign. Tick sits second in
  the key: the hot read filters `relation_id = ? AND tick = ?`, and the lab
  measured the read 22% faster with tick second than with the row-first order
  (`v6/labs/shared_frontier/out/q2d.md`).
- Support uniqueness is relation, row, and rule.
- Retractions address the same durable row identity as arrivals.

## Decisions

- Keep one typed durable table per materialized relation.
- Consolidate transient frontier and support state across relations.
- Keep specialized rule joins compiler-produced where column-aware SQL is required.
- Emit compact relation and rule metadata to both Rust and TypeScript.
- Let each static engine derive mechanical DDL and frontier operations from the shared metadata.
- Keep opaque row payloads out of shared frontier storage.
- Preserve current authored DL6, type expansion, relation identity, and boundary value semantics.

Rejected alternatives:

- One BLOB payload per frontier row: removes typed SQL access and column indexes.
- One normalized cell row per frontier column: multiplies rows and joins.
- TypeScript-only compression: leaves Rust and TypeScript consuming different lowerings.
- Compressing the generated text without changing the plan: reduces bytes on disk but retains duplicated planning and initialization work.

## Sequence

1. Add a compact `program_data` projection beside the existing `Lowered` representation.
2. Add shared frontier and support schemas to both static engines.
3. Execute arrival-only and rule-free programs through the compact path.
4. Port positive nonrecursive rule evaluation.
5. Port retraction, support-count, recursion, retention, and restart behavior.
6. Switch both emitters to compact program data after behavioral parity.
7. Remove relation-specific transient DDL and plans after no consumer reads them.

<!-- todo(perf): Replace relation-specific transient SQLite plans with shared frontier and support state while preserving one typed durable table per materialized relation. -->

## Lab receipts (v6/labs/shared_frontier, merged #374)

| claim | measured |
| --- | --- |
| tick cost, shared vs per-relation | shared faster 14-19% once a tick touches 2+ relations; up to 9% slower when it touches exactly one (`out/q2.md`) |
| where the win is | clearing a tick: one DELETE vs one per table, 12.9 ms -> 0.22 ms at 1024 relations (`out/q5.md`) |
| boot | 388.9 ms -> 70.1 ms at 1024 relations; scratch pages 3226 -> 5 (`out/q3.md`) |
| pokeapi table bill | tables 3,129 -> 783; indexes 2,348 -> 8; DDL bytes 1,682,616 -> 716,125 (`out/q1.md`) |
| frontier writes | row-at-a-time into the shared table is 11% slower than per-relation tables; 100-row multi-row INSERT is 7.4x faster. The engines batch frontier inserts per tick (`out/q4.md`) |
| compiler-time baseline | pokeapi compile measured 8m46s at b62ea5b9e against this plan's 4.14s note; re-baseline before quoting compile wall time (`REPORT.md`) |
| unmeasured | retraction identity, support-count write cost, restart (rig writes arrivals only); the behavioral matrix below covers them functionally, perf stays open |

## Verification

Behavioral CI compares old and compact lowerings for:

- keyed arrival, replacement, stale retraction, and current retraction
- unkeyed multiset arrivals
- positive joins
- negation and support counts
- recursive drain/frontier convergence
- list, option, enum, relation reference, and relation ID boundaries
- boot rows and restart
- TypeScript and Rust execution of the same emitted program data

PokeAPI CI must prove:

- 212 component names match
- 786 property names, kinds, and nullability values match
- 257 reference targets match
- generated program artifact size is reported
- compile, load, and first-tick wall times are reported separately
- rule-free schemas emit zero rule/frontier specialization records

The baseline receipts for this plan are 6,082,867 generated bytes and 4.14 seconds of compiler time after the current list-indexing work.

## Staffing

- Implementation: one compiler/runtime lane because `lower.pl`, TSV2, and `sprefa-engine-rs` share the serialized contract.
- Review: separate storage-semantics review before deleting the legacy lowering.
- Worktree: required.
- Base SHA: `1ab9cd922`.
- CI budget: focused old-versus-compact semantic matrix on each change, then full TypeScript/Rust compiler and runtime CI before switching defaults.

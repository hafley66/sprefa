# DL6 self-hosted backends and dynamically loaded monomorphized modules

## Status

- Epic: `@dl6-self-hosted-backends`
- State: deferred research
- Related foundation: `@userland-type-graph`
- Implementation authorization: none
- Current compiler seam: `plan/9 -> lowered/8 + bootstmt/3 -> emit_program/5`

## Goal

Express backend transformation rules in DL6 over queryable compiler facts. A
SQLite and Rust backend should be able to emit concrete SQL and specialized
Rust code for one program, compile that code as a loadable module, and attach
the module to a long-running server through a versioned protocol.

The generated Rust target should specialize relation rows, statements, rule
functions, schedules, storage representations, and boundary codecs. The
current `ProgramJson` interpreter remains a parity oracle during research.

## Current seam

```text
parsed and expanded DL6
        |
        v
plan/9
        |
        v
lowered/8 + bootstmt/3
        |
        +--> emit_ts.pl   --> program-specific TypeScript + generic TSV2 runtime
        |
        +--> emit_rust.pl --> embedded ProgramJson + generic Rust runtime
```

`emit_rust.pl` currently serializes a target-neutral program description and
generates a Rust function that deserializes it. Relation names, statement
plans, and schedules remain runtime data. `emit_ts.pl` generates more
program-specific source, while its tick engine remains generic.

## Target seam

```text
program.dl6 + backend.sqlite_rust.dl6
                    |
                    v
        $type + $plan + $lower rows
                    |
                    v
           compiler DL6 fixpoint
                    |
                    +--> artifact.chunk(schema.sql, Order, Text)
                    +--> artifact.chunk(module.rs, Order, Text)
                                      |
                                      v
                               rustc cdylib or wasm
                                      |
                                      v
                         versioned server/module ABI
                                      |
                                      v
                              SQLite connection
```

## Compiler relation signatures

The emitter program needs normalized source relations rather than access to
opaque `plan/9` and `lowered/8` terms.

```dl6
plan.program(Program, Name, InternMode).
plan.relation(Program, Relation, StorageName, Kind).
plan.member(Relation, Position, Name, LogicalType, StorageType).
plan.key(Relation, Position).
plan.arrival_target(Program, Relation).
plan.rule_order(Program, Position, Rule).
plan.subscription(Program, Relation).

lower.ddl(Program, Position, Sql).
lower.boot(Program, Position, Relation, Sql, Parameters).
lower.arrival(Program, Relation, Kind, AddSql, DeleteSql).
lower.edge(Program, Rule, Head, Trigger, ProjectSql, WriteSql).
lower.level(Program, Position, Head, StatementKind, Sql).
lower.delta(Program, Relation, SelectSql, DeltaTable, BoundarySql).
```

The exact normalized arities are research outputs. Every row requires a stable
semantic identity and deterministic ordering key.

## Artifact relations

```dl6
artifact.file(Target, Path).
artifact.chunk(Target, Path, Section, Position, Text).
artifact.diagnostic(Target, Severity, Code, Subject, Message).
```

The host kernel sorts chunks by `(Target, Path, Section, Position)`, verifies
key uniqueness, concatenates text, writes files, invokes external compilers,
and loads artifacts. Backend-specific branching and rendering remain DL6
rules.

## Storage annotation example

```dl6
storage(Type, sqlite, interned(text)) <-
    intern(Type, sqlite).

rust.type(Type, interned_text_id) <-
    storage(Type, sqlite, interned(text)).

sqlite.type(Type, integer) <-
    storage(Type, sqlite, interned(text)).
```

One type edge can therefore select SQLite storage, generated Rust field type,
boundary encoding, dictionary DDL, and lookup statements.

## Specialized Rust output

```rust
pub struct UserRow {
    pub id: i64,
    pub name: InternedTextId,
}

const USER_INSERT: &str = "...";

fn apply_user_arrivals(...);
fn evaluate_rule_4(...);
fn tick(...);
```

The research receipt compares this shape against the current dynamic
`ProgramJson -> Vec<RelationPlan> -> generic dispatch` path. Required counts
include generated relation structs, static statements, dynamic name lookups,
runtime plan branches, allocations, and boundary conversions.

## Dynamic module boundaries

| Boundary | SQLite connection | Data crossing | Research question |
|---|---|---|---|
| Native dynamic library with C ABI | Same in-process connection through an opaque handle or host vtable | Calls and row buffers | ABI versioning, unload safety, SQLite linkage |
| SQLite loadable extension | Same connection supplied by SQLite | SQL values and extension calls | Whether tick scheduling fits the extension lifecycle |
| WASM component | Host-owned connection exposed through handles/imports | Copied or shared linear-memory batches | Import-call and batching cost |
| Subprocess RPC | Separate connection to a shared file database | IPC messages | WAL coordination, isolation, and latency |
| Shared-memory transport | Separate connection plus shared row/message buffers | Ring or batch metadata | Ownership, wakeup, and crash recovery |

Passing Rust types such as `rusqlite::Connection` across a module boundary is
outside the contract. A native boundary uses a stable C ABI, opaque handles,
and versioned function tables.

## Module lifecycle signatures

```text
module.describe() -> { abi_version, program_hash, schema_hash, capabilities }
module.initialize(host_vtable, database_handle) -> module_handle
module.tick(module_handle, arrival_batch) -> boundary_batch
module.drain(module_handle) -> boundary_batch
module.shutdown(module_handle)
```

Artifact caching keys include the canonical program graph, projected storage
graph, backend version, ABI version, and target toolchain identity. Reload
research must define schema compatibility, migration ownership, old-module
drain, failure rollback, and artifact retention.

## Required compiler capabilities

1. Expose `plan/9`, `lowered/8`, and boot statements as finite compiler
   relations.
2. Complete `@compiler-plane-expression-parity` so emitter rules have scalar
   expressions, comparisons, ordering, and finite aggregation.
3. Add deterministic ordered artifact collection without backend syntax.
4. Represent structured target values, including JSON objects, arrays, enums,
   and optional fields, without requiring target text during every rule.
5. Run an emission-phase compiler fixpoint after planning and lowering while
   retaining the same DL6 rule semantics as earlier compiler phases.
6. Keep filesystem writes, process invocation, dynamic loading, and host ABI
   calls in the host effect boundary.

## Research task graph

```text
compiler IR relation inventory
        |
        +--> normalized $plan and $lower source relations
        |          |
        |          +--> DL6 artifact relation and deterministic collector
        |                     |
        |                     +--> ProgramJson parity emitter in DL6
        |                     |          |
        |                     |          +--> TSV2 neutral-plan consumer lab
        |                     |
        |                     +--> monomorphized SQLite and Rust backend lab
        |                                |
        |                                +--> native C ABI module lab
        |                                +--> WASM component lab
        |                                +--> RPC/shared-memory lab
        |
        +--> compiler expression parity

module labs --> measured boundary matrix --> load/reload/cache contract
```

## Receipts

- Byte or structural parity between the DL6 ProgramJson emitter and
  `emit_rust.pl` over the supported corpus.
- A generated Rust fixture with concrete row types and no runtime relation-name
  lookup on its tick path.
- SQLite statement and tick-log parity against the current Rust engine.
- Native, WASM, and RPC boundary measurements over the same arrival schedule.
- A server process that loads two program versions without recompiling itself.
- Deterministic artifact output across two clean compiler runs.

## Deferred decisions

- Canonical compiler relation arities for plan and lowered rows.
- Structured AST rows versus ordered text chunks for each backend.
- Native ABI, SQLite extension, WASM component, or process protocol as the
  first production module boundary.
- Whether TypeScript consumes the neutral program document or remains a source
  emitter.
- Database migration behavior across dynamically loaded program versions.
- Trust and sandbox policy for compiler-time DL6 emitter packages.

This epic is research-only and does not block `@userland-type-graph`.
